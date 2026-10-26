//@ codegen-matrix: standard opt
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck: --check-prefix=OPT
//@ run-call: lastIndexOf [5, 6, 5, 7], 5, 9 => 2
//@ run-call: lastIndexOf [5, 6, 5, 7], 5, 1 => 0
//@ run-call: lastIndexOf [5, 6, 5, 7], 7, 3 => 3
//@ run-call: lastIndexOf [5, 6, 5, 7], 8, 9 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: lastIndexOf [], 5, 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: lastIndexOf [5], 5, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call-fail: keepsCheck [5, 6], 5, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A backward scan starts at `from` clamped below the length. Each input of the
// start's phi stays below the length on its edge, `from` by the comparison and
// `n - 1` because `n` is not zero, so the phi does, and the index, which only
// falls from it, reads every element without a bounds check.
// OPT-LABEL: fn @lastIndexOf
// OPT-NOT: mstore 32, 50
// OPT: returndata
//
// An input that only one edge bounds leaves the phi unbounded.
// OPT-LABEL: fn @keepsCheck
// OPT: mstore 32, 50
contract JoinPhiRelations {
    function lastIndexOf(uint256[] memory a, uint256 needle, uint256 from)
        external
        pure
        returns (uint256)
    {
        uint256 n = a.length;
        if (n == 0) return type(uint256).max;
        uint256 i = from < n ? from : n - 1;
        while (a[i] != needle) {
            if (i == 0) return type(uint256).max;
            --i;
        }
        return i;
    }

    function keepsCheck(uint256[] memory a, uint256 needle, uint256 from)
        external
        pure
        returns (uint256)
    {
        uint256 n = a.length;
        if (n == 0) return type(uint256).max;
        uint256 i = from < n ? from : n;
        while (a[i] != needle) {
            if (i == 0) return type(uint256).max;
            --i;
        }
        return i;
    }
}
