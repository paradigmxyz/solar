//@compile-flags: -Zcodegen --emit=mir

// Dynamic memory allocations must revert with Panic(0x41) when the requested
// size overflows while computing the padded byte length, element byte length,
// total allocation size, or free-memory pointer bump.
contract MemoryAllocationPanic {
    function makeBytes(uint256 n) external pure returns (uint256) {
        bytes memory b = new bytes(n);
        return b.length;
    }

    function makeArray(uint256 n) external pure returns (uint256) {
        uint256[] memory a = new uint256[](n);
        return a.length;
    }
}
