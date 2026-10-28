//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: untagged 65536 => true, 101
//@ run-call: untaggedReload 65536 => true, 101
//@ run-call: consumed 65536 => true
//@ run-call: disjointStore 101 => 101

contract Test {
    // The untagged version of the rejected retained-pointer case keeps the first payload.
    function untagged(uint256 base) external pure returns (bool different, uint256 result) {
        assembly { mstore(0x40, base) }
        bytes32 first;
        {
            bytes memory encoded = abi.encode(uint256(101));
            first = keccak256(encoded);
        }
        bytes memory next = abi.encode(uint256(103));
        different = first != keccak256(next);
        assembly { result := mload(add(base, 32)) }
    }

    // A memory round trip does not hide the first encoding's payload from the retained base.
    function untaggedReload(uint256 base) external pure returns (bool different, uint256 result) {
        assembly {
            mstore(0, base)
            mstore(32, 0)
            mstore(64, mload(0))
        }
        bytes32 first;
        { bytes memory encoded = abi.encode(uint256(101)); first = keccak256(encoded); }
        bytes memory next = abi.encode(uint256(103));
        different = first != keccak256(next);
        assembly { result := mload(add(base, 32)) }
    }

    // Installing an address is fine when no alias survives the block.
    function consumed(uint256 base) external pure returns (bool different) {
        assembly { mstore(0x40, base) }
        bytes32 first;
        /// @custom:solar-scratch
        { first = keccak256(abi.encode(uint256(101))); }
        bytes memory next = abi.encode(uint256(103));
        different = first != keccak256(next);
    }

    // Typed stores into a fresh object do not install their scalar values into the FMP.
    function disjointStore(uint256 base) external pure returns (uint256) {
        uint256[] memory kept = new uint256[](1);
        kept[0] = base;
        /// @custom:solar-scratch
        { bytes32 hash = keccak256(abi.encode(uint256(101))); }
        return kept[0];
    }
}
