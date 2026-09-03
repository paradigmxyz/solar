// External parameters decoded into memory whose ABI length is too large to
// allocate, or whose head offset points at a word that reads as such a
// length. solc reverts with Panic(0x41); solar reverts with empty returndata.
// Calldata-typed parameters agree (both revert with empty returndata).
// Example calldata after the selector: [0x20, 0xffffffffffffffff].
contract AbiDecodeMemoryOversizedLength {
    struct S {
        uint256 a;
        uint256[] arr;
    }

    function u256Array(uint256[] memory a) external pure returns (uint256) {
        return a.length;
    }

    function u8Array(uint8[] memory a) external pure returns (uint256) {
        return a.length;
    }

    function str(string memory s) external pure returns (uint256) {
        return bytes(s).length;
    }

    function bytesMem(bytes memory b) external pure returns (uint256) {
        return b.length;
    }

    function structMem(S memory s) external pure returns (uint256) {
        return s.a + s.arr.length;
    }

    function nested(uint256[][] memory a) external pure returns (uint256) {
        return a.length;
    }

    // Agrees with solc.
    function u256ArrayCalldata(uint256[] calldata a) external pure returns (uint256) {
        return a.length;
    }
}
