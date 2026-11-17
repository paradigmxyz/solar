//@ compile-flags: -Onone -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The block that enters a `break` may itself only jump on, and the forwarding chain of the stack
// scheduler must not walk through the jump of the statement.

// CHECK-LABEL: (runtime) ===
// CHECK: [entry]:{{.*}}function f(
// CHECK: push bb{{[0-9]+}} !metadata(keep_with_next)
// CHECK-NEXT: jump
contract C {
    function f(uint256 n) external pure returns (uint256 r) {
        for (;;) {
            r = n;
            break;
        }
    }
}
