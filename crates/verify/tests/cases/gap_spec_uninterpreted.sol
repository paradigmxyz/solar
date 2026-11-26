pragma solidity ^0.8.20;
// A pure function whose body is more than a single `return` is uninterpreted in a spec: equal
// arguments give equal results, and nothing more is known. A claim about what it computes then
// fails with a spurious counterexample, so the spec that calls it gets a warning saying so.
contract SpecCalls {
    function payout(uint256 a, uint256 f, uint256 b) public pure returns (uint256) {
        uint256 fee = (a * f) / 10000;
        return a - fee + b;
    }

    function single(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    // expect: `payout` is uninterpreted in this spec, because its body is more than a single `return`
    /// @custom:requires a <= c && f <= 10000
    /// @custom:ensures payout(a, f, b) <= payout(c, f, b)
    function bad_monotone(uint256 a, uint256 c, uint256 f, uint256 b) external pure {}

    // Equal arguments still give equal results.
    // expect: state it with a body-level `@custom:assert`, or make `payout` a single-expression function
    /// @custom:ensures payout(a, f, b) == payout(a, f, b)
    function ok_congruent(uint256 a, uint256 f, uint256 b) external pure {}

    /// @custom:ensures r == single(a, b)
    function ok_single(uint256 a, uint256 b) external pure returns (uint256 r) {
        r = a + b;
    }
}
