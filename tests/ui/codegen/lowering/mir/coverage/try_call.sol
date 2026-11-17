//@compile-flags: -O none -Zinstrument-coverage -Zdump=mir
//@filecheck: --enable-var-scope

// A `try` counts the calls that returned, right after the call, and each clause counts against
// them, so a revert while the arguments are evaluated or before the call runs is no failure of a
// clause.
contract Callee {
    function g(uint256 a) external pure returns (uint256) {
        return a;
    }
}

contract TryCall {
    // CHECK-LABEL: fn @f(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: coverage 2
    // CHECK: staticcall
    // CHECK-NEXT: coverage 3
    // CHECK-NEXT: jumpi
    function f(Callee c) external pure returns (bool ok) {
        try c.g(1) returns (uint256) {
            ok = true;
        } catch {
            ok = false;
        }
    }
}
