//@ compile-flags: -Onone -Zdump=mir-final
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// `first` returns one word of the two that `second` returns, so its call is no tail call, and the
// `break` after it keeps its block at `-O none`: only a call whose whole result is returned stays
// in tail position.
contract C {
    // CHECK-LABEL: ; function first(
    // CHECK: icall @second
    // CHECK: jump [[BREAK:bb[0-9]+]]
    // CHECK: [[BREAK]]:
    // CHECK-NEXT: ret
    function first(bytes calldata d, uint256 n) internal pure returns (uint256 r) {
        for (;;) {
            r = second(d, n - 1).length;
            break;
        }
    }

    function second(bytes calldata d, uint256 n) internal pure returns (bytes calldata s) {
        if (n == 0) return d;
        if (first(d, n - 1) == 99) return d[1:];
        return d;
    }

    function len(bytes calldata d, uint256 n) external pure returns (uint256) {
        return first(d, n);
    }
}
