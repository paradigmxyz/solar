//@ codegen-matrix: standard
//@ run-call: straight 3 => 9
//@ run-call: straight 0 => 0
//@ run-call: pair 9, 0 => 11
//@ run-call: pair 0, 9 => 12
//@ run-call: pair 0, 0 => 13
//@ run-call: pair 9, 9 => 11

// `@custom:solar-cold` says a function is rarely run, not that it never returns. Code after a call
// to it must still run, and a call to it must not count as an abort: `choose` ends its two tests
// in blocks that call `bump` and return different values, which a function that cannot return
// would let `merge-aborts` join into one test.

contract C {
    uint256 total;

    /// @custom:solar-cold
    function bump(uint256 x) internal returns (uint256) {
        for (uint256 i; i < x; ++i) {
            total += 1;
        }
        return total * 2;
    }

    function straight(uint256 x) public returns (uint256) {
        uint256 y = bump(x);
        return y + x;
    }

    /// @custom:solar-inline never
    function choose(uint256 a, uint256 b) internal returns (uint256) {
        if (a > 5) {
            bump(a);
            return 1;
        }
        if (b > 5) {
            bump(a);
            return 2;
        }
        return 3;
    }

    function pair(uint256 a, uint256 b) public returns (uint256) {
        return choose(a, b) + 10;
    }
}
