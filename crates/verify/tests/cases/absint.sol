// args: --infer-absint
// What a write or a call must drop is pinned without a solver by the `absint_candidates` test.
pragma solidity ^0.8.20;

/// Loops without written invariants: --infer-absint infers them by abstract interpretation, and
/// each inferred invariant is proved like a written one.
library Absint {
    /// relational bound i <= n
    /// @custom:ensures result == n
    function ok_bound(uint256 n) internal pure returns (uint256) {
        uint256 i = 0;
        while (i < n) { i++; }
        return i;
    }

    /// processed-prefix summary: forall j < i, xs[j] <= m, and m is one of them
    /// @custom:requires xs.length > 0
    /// @custom:ensures forall uint256 j :: j < xs.length ==> xs[j] <= m
    /// @custom:ensures exists uint256 j :: j < xs.length && xs[j] == m
    function ok_prefix_max(uint256[] memory xs) internal pure returns (uint256 m) {
        m = xs[0];
        for (uint256 i = 1; i < xs.length; i++) {
            if (xs[i] > m) m = xs[i];
        }
    }

    /// branch joins through continue
    /// @custom:ensures s <= n
    function ok_join(uint256 n) internal pure returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) {
            if (i % 2 == 0) continue;
            s++;
        }
    }

    /// unchecked wraparound: i += 1 wraps past the type's maximum, so i >= start is not an
    /// invariant (math-integer reasoning would infer it)
    /// @custom:ensures result >= start
    function bad_wrap(uint256 n, uint256 start) internal pure returns (uint256 i) {
        i = start;
        while (i != n) {
            unchecked { i += 1; }
        }
    }

    /// unchecked increments that cannot wrap keep their bounds
    /// @custom:ensures result == n
    function ok_unchecked_inc(uint256 n) internal pure returns (uint256 i) {
        while (i < n) {
            unchecked { ++i; }
        }
    }
}
