//@ codegen-matrix: standard
//@ run-call: fee 10, 4194311, true, 0 => 7, 7
//@ run-call: fee 10, 0, false, 3 => 13, 10
//@ run-call: fee 10, 4194311, true, 2 => 9, 7

// `f` is a phi result of a planned join with no definition-time store. It
// feeds the second join's phi on the direct edge and stays live past it: the
// spill before that branch must not treat it as carried, because the edge
// renames it into the phi result. Reduced from Uniswap v4's
// `PoolTest::test_fuzz_swap`.
contract JoinCarryPhiSourceLive {
    // Recursive, so the inliner leaves the call in place.
    function helper(uint256 n, uint256 f) internal pure returns (uint256) {
        if (n == 0) return f;
        return helper(n - 1, f + 1);
    }

    function fee(uint256 base, uint256 over, bool useOver, uint256 pf)
        external
        pure
        returns (uint256 a, uint256 b)
    {
        uint256 f = useOver ? (over & 0x3fffff) : base;
        uint256 s = pf == 0 ? f : helper(pf, f);
        a = s;
        b = f & 0xffffff;
    }
}
