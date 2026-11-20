//@ compile-flags: -Ogas --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ normalize-stdout-test: "(?s).+" -> ""
//@ filecheck:

// A `return` or `leave` that follows a call as a statement of its own does not enclose the call.
// The tail call's argument setup maps to the call, and the callee push and jump that leave the
// function map to that statement, as the jump of a plain `return` does. The span of the jump is the offset of
// `return;` in `g`. When layout falls the tail call through into its callee,
// no jump is left and the statement has no entry, as a statement that emits no code has none.
// `-Onone` folds no call into a tail call, and the tags keep `-Ogas` from inlining the functions.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [invoke=g
// CHECK-NEXT: push 1 !metadata(span=[[G:[0-9]+..[0-9]+]])
// CHECK-NEXT: swap 2 !metadata(span=[[G]])
// CHECK-NEXT: swap 1 !metadata(span=[[G]])
// CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=1653..1660, exit=return)

// The implicit return that ends a function maps to the closing brace, which is no statement of its
// own, so the jump keeps the call, as the tail-call instruction of LLVM does.
// CHECK: [invoke=i
// CHECK-NEXT: push 2 !metadata(span=[[I:[0-9]+..[0-9]+]])
// CHECK-NEXT: swap 2 !metadata(span=[[I]])
// CHECK-NEXT: swap 1 !metadata(span=[[I]])
// CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=[[I]], exit=return)

contract C {
    uint256 public s;

    /// @custom:solar-inline never
    function f(uint256 a, uint256 b) internal {
        s = a + b;
    }

    /// @custom:solar-inline never
    function g(uint256 x) internal {
        f(x, 1);
        return;
    }

    /// @custom:solar-inline never
    function i(uint256 x) internal {
        f(x, 2);
    }

    function a(uint256 y) external {
        g(y);
        i(y);
    }
}
