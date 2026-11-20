//@ compile-flags: -O none -Zdump=evm-ir-runtime
//@ filecheck:
//@ run-call: f 0 => 1
//@ run-call: f 7 => 7

// `-O none` follows clang's `-O0`: its `cfg-simplify` merges no block, so the selector wrapper stays
// a jump-only block between the dispatch and the function.
contract NoneCheapCleanup {
    // CHECK-LABEL: @module NoneCheapCleanup_runtime
    // CHECK: eq
    // CHECK-NEXT: push [[WRAPPER:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[WRAPPER]]{{( \[[a-z, ]+\])?}}:
    // CHECK-NEXT: jump bb
    // CHECK: return
    function f(uint256 x) public pure returns (uint256 r) {
        if (x == 0) return 1;
        r = x;
    }
}
