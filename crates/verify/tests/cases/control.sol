pragma solidity ^0.8.20;
contract Control {
    /// @custom:ensures result == n
    function ok_while(uint256 n) public pure returns (uint256) {
        uint256 i = 0;
        /// @custom:invariant i <= n
        while (i < n) { i++; }
        return i;
    }
    /// No invariant is written: inference finds `i <= n`.
    /// @custom:ensures result == n
    function ok_no_invariant(uint256 n) public pure returns (uint256) {
        uint256 i = 0;
        while (i < n) { i++; }
        return i;
    }
    /// Inference cannot prove a false postcondition: the loop exits with `i == n`.
    /// @custom:ensures result == n + 1
    function bad_no_invariant(uint256 n) public pure returns (uint256) {
        uint256 i = 0;
        while (i < n) { i++; }
        return i;
    }
    /// `s == 0` holds on entry but the body does not preserve it, so Houdini drops it and the
    /// postcondition it would have given fails.
    /// @custom:ensures result == 0
    function bad_noninductive_candidate(uint256 n) public pure returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) { s += i; }
    }
    /// @custom:ensures result <= 10
    function ok_break(uint256 n) public pure returns (uint256 i) {
        /// @custom:invariant i <= 10
        for (i = 0; i < n; i++) {
            if (i == 10) break;
        }
    }
    /// @custom:ensures result == 5
    function bad_break(uint256 n) public pure returns (uint256 i) {
        /// @custom:invariant i <= 10
        for (i = 0; i < n; i++) {
            if (i == 10) break;
        }
    }
    /// @custom:ensures s <= n
    function ok_continue(uint256 n) public pure returns (uint256 s) {
        /// @custom:invariant i <= n && s <= i
        for (uint256 i = 0; i < n; i++) {
            if (i % 2 == 0) continue;
            s++;
        }
    }
    /// @custom:ensures result == (x > 5 ? 1 : 2)
    function ok_early_return(uint256 x) public pure returns (uint256) {
        if (x > 5) { return 1; }
        return 2;
    }
    function bad_decreases(uint256 n) public pure {
        uint256 i = 0;
        /// @custom:invariant i <= n
        /// @custom:decreases i
        while (i < n) { i++; }
    }
    function bad_assert_ghost(uint256 x) public pure {
        /// @custom:assert x > 0
        x = x;
    }
    function ok_require_assume(uint256 x) public pure {
        require(x > 3);
        /// @custom:assert x > 2
        x = x;
    }
}
