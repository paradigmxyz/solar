//@ compile-flags: -Onone -Zdump=mir-final
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// `first` returns the pointer of the slice that `second` returns in its second field, not the
// first field of the call's result, so its call is no tail call, and the `break` after it keeps
// its block at `-O none`.
contract C {
    // CHECK-LABEL: ; function first(
    // CHECK: icall @second
    // CHECK-NOT: ret
    // CHECK: jump [[BREAK:bb[0-9]+]]
    // CHECK: [[BREAK]]:
    function first(bytes calldata d, uint256 n) internal pure returns (uint256, uint256) {
        uint256 a;
        bytes calldata s = d;
        for (;;) {
            (a, s) = second(d, n);
            break;
        }
        return (a, s.length);
    }

    function second(bytes calldata d, uint256 n) internal pure returns (uint256, bytes calldata) {
        if (n == 0) return (0, d);
        (uint256 a, ) = first(d, n - 1);
        return (a, d);
    }

    function sum(bytes calldata d, uint256 n) external pure returns (uint256, uint256) {
        return first(d, n);
    }
}
