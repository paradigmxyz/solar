//@ codegen-matrix: standard
//@ run-call: read 2, [5, 6, 7, 8], 3 => 21
//@ run-call-fail: read 4, [5, 6, 7, 8], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A static calldata array stays a calldata offset instead of one argument per
// head word, so the largest array calldata allows compiles without a table of
// 2^27 entries. A scalar behind it loads its word directly.
contract C {
    function huge(uint256[134217727] calldata a) external pure returns (uint256) {
        return a[134217726];
    }

    function hugeThenIndex(uint256[134217727] calldata a, uint256 i)
        external
        pure
        returns (uint256)
    {
        return a[i] + i;
    }

    function read(uint256 i, uint256[4] calldata a, uint256 j) external pure returns (uint256) {
        return a[i] * j;
    }
}
