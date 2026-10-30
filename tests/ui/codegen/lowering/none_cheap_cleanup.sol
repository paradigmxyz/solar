//@ compile-flags: -O none -Zdump=evm-ir-runtime
//@ filecheck:
//@ run-call: f 0 => 1
//@ run-call: f 7 => 7

// `-O none` runs the first `cfg-simplify` of the lowering pipeline and of the EVM IR pipeline, so
// the selector wrapper and the `if` join are merged into their predecessors instead of left as
// jump-only blocks.
contract NoneCheapCleanup {
    // CHECK-LABEL: @module NoneCheapCleanup_runtime
    // CHECK: eq
    // CHECK-NEXT: push [[WRAPPER:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[WRAPPER]]:
    // CHECK-NEXT: push 36
    // CHECK-NOT: jump bb
    // CHECK: return
    function f(uint256 x) public pure returns (uint256 r) {
        if (x == 0) return 1;
        r = x;
    }
}
