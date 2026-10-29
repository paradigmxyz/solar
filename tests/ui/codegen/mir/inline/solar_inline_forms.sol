//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// A bare `@custom:solar-inline` favors a function: the gas objective clones it where the clone
// adds at most as many bytes again as the call it removes, whatever gas that saves, so a helper
// that is called twice and that the inliner keeps shared is cloned. `always` clones every legal
// call, and `never` clones none. All four helpers below have one body, called twice each.
contract C {
    function plain(uint256 a, uint256 b) internal pure returns (uint256) {
        return (a * 31 + b) ^ (a >> 3);
    }

    /// @custom:solar-inline
    function hint(uint256 a, uint256 b) internal pure returns (uint256) {
        return (a * 31 + b) ^ (a >> 3);
    }

    /// @custom:solar-inline always
    function always(uint256 a, uint256 b) internal pure returns (uint256) {
        return (a * 31 + b) ^ (a >> 3);
    }

    /// @custom:solar-inline never
    function never(uint256 a, uint256 b) internal pure returns (uint256) {
        return (a * 31 + b) ^ (a >> 3);
    }

    // CHECK-LABEL: fn @f{{.*}}()
    // CHECK: icall @plain
    // CHECK: icall @plain
    function f(uint256 x, uint256 y) external pure returns (uint256) {
        return plain(x, y) + plain(y, x);
    }

    // CHECK-LABEL: fn @g{{.*}}()
    // CHECK-NOT: icall
    // CHECK: ret
    function g(uint256 x, uint256 y) external pure returns (uint256) {
        return hint(x, y) + hint(y, x);
    }

    // CHECK-LABEL: fn @h{{.*}}()
    // CHECK-NOT: icall
    // CHECK: ret
    function h(uint256 x, uint256 y) external pure returns (uint256) {
        return always(x, y) + always(y, x);
    }

    // CHECK-LABEL: fn @i{{.*}}()
    // CHECK: icall @never
    // CHECK: icall @never
    function i(uint256 x, uint256 y) external pure returns (uint256) {
        return never(x, y) + never(y, x);
    }
}
