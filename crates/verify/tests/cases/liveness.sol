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
        /// @custom:decreases n - i
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

interface ILiveCallee {
    function g() external;
    /// @custom:succeeds-if true
    function h() external;
}

/// Liveness across calls: a contracted internal callee may revert unless its own succeeds_if holds,
/// and an external call may revert unless a try catches it or its interface says it returns.
contract LiveCalls {
    uint256 x;
    ILiveCallee i;
    /// @custom:ensures x == old(x)
    function _c() internal view { if (x == 5) revert(); }
    /// @custom:ensures x == old(x)
    /// @custom:succeeds_if x != 5
    function _d() internal view { if (x == 5) revert(); }
    /// @custom:succeeds_if true
    function bad_contracted() external view { _c(); }
    /// @custom:succeeds_if x != 5
    function ok_contracted() external view { _d(); }
    /// @custom:succeeds_if true
    function bad_contracted_outside() external view { _d(); }
    /// @custom:succeeds_if true
    function bad_external() external { i.g(); }
    /// @custom:succeeds_if true
    function ok_try() external { try i.g() {} catch {} }
    /// @custom:succeeds_if true
    function ok_iface_live() external { i.h(); }
}

/// Reverts the executor does not see as values: liveness must still count them.
contract LiveGaps {
    uint256 n;
    /// @custom:succeeds_if true
    function bad_bytes_index(bytes calldata b) external pure returns (bytes1) { return b[0]; }
    /// @custom:succeeds_if b.length > 0
    function ok_bytes_index(bytes calldata b) external pure returns (bytes1) { return b[0]; }
    /// @custom:succeeds_if true
    function bad_slice(bytes calldata b) external pure returns (uint256) { return b[2:].length; }
    /// @custom:succeeds_if true
    function bad_new_array(uint256 k) external pure returns (uint256) { return new uint256[](k).length; }
    /// @custom:succeeds_if k < 1000
    function ok_new_array(uint256 k) external pure returns (uint256) { return new uint256[](k).length; }
    /// @custom:succeeds_if true
    function bad_decode(bytes calldata b) external pure returns (uint256) { return abi.decode(b, (uint256)); }
    /// @custom:succeeds_if address(this).balance >= 1
    function bad_transfer() external { payable(msg.sender).transfer(1); }
    function _inc(uint8 x) internal pure returns (uint8) { return x + 1; }
    /// @custom:succeeds_if true
    function bad_unchecked_callee(uint8 x) external pure returns (uint8) { unchecked { return _inc(x); } }
    /// @custom:succeeds_if true
    function ok_unchecked_own(uint8 x) external pure returns (uint8) { unchecked { return x + 1; } }
}

/// Liveness needs termination: a loop without a decreases clause, and recursion, may never return.
contract LiveTermination {
    /// @custom:succeeds_if true
    function bad_spin() external pure returns (uint256 i) {
        /// @custom:invariant true
        for (;;) { i = i; }
    }
    /// @custom:succeeds_if true
    function bad_stuck(uint256 k) external pure returns (uint256 i) {
        /// @custom:invariant i <= k
        while (i < k) { i = i; }
    }
    /// @custom:succeeds_if true
    function ok_counted(uint256 k) external pure returns (uint256 i) {
        /// @custom:invariant i <= k
        /// @custom:decreases k - i
        while (i < k) { i = i + 1; }
    }
    /// @custom:succeeds_if true
    /// @custom:ensures true
    function bad_self(uint256 x) public pure returns (uint256) { return bad_self(x); }
    /// @custom:succeeds_if true
    /// @custom:ensures true
    function bad_ping(uint256 x) public pure returns (uint256) { return bad_pong(x); }
    /// @custom:succeeds_if true
    /// @custom:ensures true
    function bad_pong(uint256 x) public pure returns (uint256) { return bad_ping(x); }
}
