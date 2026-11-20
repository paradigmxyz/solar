//@ compile-flags: -Onone -Zdump=mir-final
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// `first` returns the first two words of the three that `second` returns, so its call is no tail
// call, and the `break` after it keeps its block at `-O none`.
contract C {
    // CHECK-LABEL: ; function first(
    // CHECK: icall @second
    // CHECK-NOT: ret
    // CHECK: jump [[BREAK:bb[0-9]+]]
    // CHECK: [[BREAK]]:
    function first(uint256 n) internal pure returns (uint256, uint256) {
        uint256 a;
        uint256 b;
        for (;;) {
            (a, b, ) = second(n);
            break;
        }
        return (a, b);
    }

    function second(uint256 n) internal pure returns (uint256, uint256, uint256) {
        if (n == 0) return (0, 0, 0);
        (uint256 a, uint256 b) = first(n - 1);
        return (a, b, 0);
    }

    function sum(uint256 n) external pure returns (uint256, uint256) {
        return first(n);
    }
}
