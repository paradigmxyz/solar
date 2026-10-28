//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: untagged 64, 7, 9 => 7, false
//@ run-call: consumedBefore 64, 7 => 7
//@ run-call: readAfter 64, 7, 9 => 9
//@ run-call: disjoint 7 => 0
//@ run-call: returnedScalar 64, 7 => 7

contract Test {
    // The tagged version is rejected in solar_scratch_dynamic_fmp.sol. Without reclamation,
    // the pointer still observes the first encoding even after the next allocation.
    // CHECK-LABEL: fn @untagged(
    // CHECK: mload arg0
    // CHECK: ret
    function untagged(uint256 slot, uint256 a, uint256 b) external pure returns (uint256 result, bool same) {
        uint256 pointer;
        assembly { pointer := mload(slot) }
        bytes32 first;
        {
            bytes memory encoded = abi.encode(a);
            first = keccak256(encoded);
        }
        bytes memory next = abi.encode(b);
        bytes32 second = keccak256(next);
        assembly { result := mload(add(pointer, 32)) }
        same = first == second;
    }

    // The dynamic pointer is used only before the block. The loaded scalar follows its store,
    // rather than becoming another possible free pointer just because its address is unknown.
    function consumedBefore(uint256 slot, uint256 x) external pure returns (uint256 result) {
        assembly {
            let pointer := mload(slot)
            mstore(pointer, x)
            result := mload(pointer)
            mstore(0x40, add(pointer, 32))
        }
        /// @custom:solar-scratch
        { require(keccak256(abi.encode(x)) != bytes32(0)); }
    }

    function readAfter(uint256 slot, uint256 a, uint256 b) external pure returns (uint256 result) {
        /// @custom:solar-scratch
        { require(keccak256(abi.encode(a)) != bytes32(0)); }
        uint256 pointer;
        assembly { pointer := mload(slot) }
        bytes memory next = abi.encode(b);
        require(keccak256(next) != bytes32(0));
        assembly { result := mload(add(pointer, 32)) }
    }

    function disjoint(uint256 x) external pure returns (uint256 result) {
        // The zero slot is adjacent to, but disjoint from, the FMP word.
        assembly { result := mload(add(0x20, 0x40)) }
        /// @custom:solar-scratch
        { require(keccak256(abi.encode(x)) != bytes32(0)); }
    }

    /// @custom:solar-noinline
    function storedScalar(uint256 slot, uint256 x) internal pure returns (uint256 result) {
        assembly {
            let pointer := mload(slot)
            mstore(pointer, x)
            result := mload(pointer)
            mstore(0x40, add(pointer, 32))
        }
    }

    function returnedScalar(uint256 slot, uint256 x) external pure returns (uint256 result) {
        result = storedScalar(slot, x);
        /// @custom:solar-scratch
        { require(keccak256(abi.encode(x)) != bytes32(0)); }
    }
}
