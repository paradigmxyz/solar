pragma solidity ^0.8.20;
/// succeeds_if: liveness, the function returns normally whenever P holds.
library Live {
    error ZeroFill();
    /// @custom:succeeds_if a > 0 && b > 0
    function ok_div(uint256 a, uint256 b) internal pure returns (uint256) { return a / b; }
    /// @custom:succeeds_if b > 0
    function bad_div(uint256 a, uint256 b) internal pure returns (uint256 r) {
        r = a / b;
        if (r == 0) revert ZeroFill();
    }
    /// @custom:succeeds_if x <= 100
    function ok_add(uint256 x) internal pure returns (uint256) { return x + 1; }
    /// @custom:succeeds_if true
    function bad_overflow(uint256 x) internal pure returns (uint256) { return x + 1; }
    /// @custom:succeeds_if bid && !exactIn && amount > 0
    function bad_dropped(bool bid, bool exactIn, uint256 amount) internal pure returns (uint256 amountIn, uint256 amountOut) {
        if (bid && !exactIn) { amountOut = amount; } else { amountIn = amount; amountOut = amount; }
        if (amountIn == 0 || amountOut == 0) revert ZeroFill();
    }
}

/// Liveness through loops: revert points are proved unreachable over the loop's invariant abstraction.
library LiveLoop {
    error E();
    /// @custom:succeeds_if n <= 10
    function ok_loop(uint256 n) internal pure returns (uint256 s) {
        /// @custom:invariant i <= n && s == i
        for (uint256 i; i < n; ++i) { s += 1; }
        if (s > 10) revert E();
    }
    /// @custom:succeeds_if n <= 10
    function bad_loop(uint256 n) internal pure returns (uint256 s) {
        /// @custom:invariant i <= n && s == i
        for (uint256 i; i < n; ++i) { s += 1; if (s == 5) revert E(); }
    }
    /// @custom:succeeds_if n > 0
    function bad_require(uint256 n) internal pure returns (uint256) { require(n > 1); return n; }
}
