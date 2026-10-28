//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: copied 7, 9 => 7, false
//@ run-call: identity 7, 9 => 7, false
//@ run-call: forwarded 7, 9 => 7, false
//@ run-call: disjointCopies 7 => 123
//@ run-call: emptyCopies 7 => 123

contract Test {
    // Without reclamation the pointer keeps observing the first allocation.
    // CHECK-LABEL: fn @copied(
    // CHECK: mcopy
    // CHECK: ret
    function copied(uint256 a, uint256 b) external pure returns (uint256 result, bool same) {
        uint256 pointer;
        assembly {
            mcopy(0, 0x40, 32)
            pointer := mload(0)
        }
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

    function identity(uint256 a, uint256 b) external view returns (uint256 result, bool same) {
        uint256 pointer;
        assembly {
            if iszero(staticcall(gas(), 4, 0x40, 32, 0, 32)) { revert(0, 0) }
            pointer := mload(0)
        }
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

    function forwarded(uint256 a, uint256 b) external pure returns (uint256 result, bool same) {
        uint256 pointer;
        assembly {
            let slot := add(32, 32)
            mstore(slot, 0x400)
            pointer := mload(slot)
        }
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

    function disjointCopies(uint256 a) external view returns (uint256 result) {
        assembly {
            mstore(0, 123)
            mcopy(32, 0, 32)
            if iszero(staticcall(gas(), 4, 32, 32, 0, 32)) { revert(0, 0) }
            result := mload(0)
        }
        /// @custom:solar-scratch
        { require(keccak256(abi.encode(a)) != bytes32(0)); }
    }

    function emptyCopies(uint256 a) external view returns (uint256 result) {
        assembly {
            mstore(0, 123)
            mcopy(0, 64, 0)
            if iszero(staticcall(gas(), 4, 64, 0, 0, 32)) { revert(0, 0) }
            result := mload(0)
        }
        /// @custom:solar-scratch
        { require(keccak256(abi.encode(a)) != bytes32(0)); }
    }
}
