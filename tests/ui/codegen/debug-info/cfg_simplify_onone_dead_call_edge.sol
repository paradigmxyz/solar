//@ compile-flags: -Onone -Zdump=mir-final
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A call in a branch that `-O none` folds away does not make `f` and `g` recursive, so the `break`
// that follows `f`'s call to `g` keeps its block: only a real recursive call stays in tail position.
contract C {
    // CHECK-LABEL: ; function f(
    // CHECK: icall @g
    // CHECK-NEXT: jump [[BREAK:bb[0-9]+]]
    // CHECK: [[BREAK]]:
    // CHECK: ret
    function f(uint256 n) internal pure returns (uint256) {
        uint256 r;
        for (;;) {
            r = g(n);
            break;
        }
        return r;
    }

    function g(uint256 n) internal pure returns (uint256) {
        if (false) return f(n);
        return n;
    }

    function h(uint256 n) external pure returns (uint256) {
        return f(n);
    }
}
