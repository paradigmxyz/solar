//@ compile-flags: -Onone -Zdump=mir-final
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// `rec` returns the two words of its call swapped, so the call is no tail call, and the `break`
// after it keeps its block at `-O none`: only a call whose result is returned as it is stays in
// tail position.
contract C {
    // CHECK-LABEL: ; function rec(
    // CHECK: icall @rec
    // CHECK-NOT: ret
    // CHECK: jump [[BREAK:bb[0-9]+]]
    // CHECK: [[BREAK]]:
    function rec(uint256 n) internal pure returns (uint256, uint256) {
        if (n == 0) return (0, 0);
        uint256 a;
        uint256 b;
        for (;;) {
            (a, b) = rec(n - 1);
            break;
        }
        return (b, a);
    }

    function sum(uint256 n) external pure returns (uint256, uint256) {
        return rec(n);
    }
}
