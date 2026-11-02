//@ compile-flags: -Osize
//@ run-call: f 5, 1, 10 => 5
//@ run-call: f 0, 1, 10 => 1
//@ run-call: f 99, 1, 10 => 10
//@ run-call: f 1461501637330902918203684832716283019655932542976, 0, 1461501637330902918203684832716283019655932542976 => 0
//@ run-call-fail: f 5, 10, 1 => Error("bad bounds")

// The return address the inlined `clamp` pushes is carried into its other branches by
// conditional jumps, so only the jumps that return through it bound the depth its block is
// entered at, and that block keeps the shorter mask recipe.
contract DynamicEntry {
    function f(uint256 x, uint256 lo, uint256 hi) external pure returns (uint128) {
        return uint128(clamp(x, lo, hi));
    }

    function clamp(uint256 x, uint256 lo, uint256 hi) internal pure returns (uint256) {
        require(lo <= hi, "bad bounds");
        if (x < lo) return lo;
        if (x > hi) return hi;
        return x;
    }
}
