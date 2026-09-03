// Allocating a memory array whose length exceeds the allocator limit must
// revert with Panic(0x41). solc does; solar reverts with Panic(0x32).
// Source: testdata/solidity/test/libsolidity/semanticTests/array/create_memory_array_too_large.sol
contract MemoryArrayTooLarge {
    function f() external pure returns (uint256) {
        uint256 l = 2**256 / 32;
        uint256[] memory x = new uint256[](l);
        x[1] = 42;
        return x[1];
    }

    // Agrees with solc after ba539e44a: g() from the same solc test.
    function g() external pure returns (uint256) {
        uint256 l = 2**256 / 2 + 1;
        uint8[] memory x = new uint8[](l);
        x[2] = 42;
        return x[2];
    }

    // These still differ, but through CODEGEN-003 (docs/SOLC_DIVERGENCE.md),
    // not the allocation check: the inline literal `2**256 / 32` is lowered
    // with wrapping EVM arithmetic and evaluates to 0, so solar allocates an
    // empty array where solc panics with 0x41. The same lengths through a
    // local or a constant agree.
    function nested() external pure returns (uint256) {
        uint256[][] memory a = new uint256[][](2**256 / 32);
        return a.length;
    }

    function structs() external pure returns (uint256) {
        S[] memory a = new S[](2**256 / 64);
        return a.length;
    }

    function str() external pure returns (uint256) {
        string memory s = new string(2**256 / 32);
        return bytes(s).length;
    }

    struct S {
        uint256 a;
        uint256 b;
    }
}
