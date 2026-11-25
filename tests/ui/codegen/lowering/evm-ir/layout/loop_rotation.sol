//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=disasm-runtime
//@[ir] filecheck:
//@ run-call: route 1000, 0 => 1000
//@ run-call: route 1000, 3 => 991
//@ run-call-fail: route 0x8000000000000000000000000000000000000000000000000000000000000000, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract LoopRotation {
    // CHECK-LABEL: (runtime) ===
    // The latch falls into the header, whose branch closes the loop, so an
    // iteration runs no `JUMP`. The overflow check branches to the cold panic
    // block, so the hot path falls through it.
    // CHECK: {{JUMP +; -> bb[0-9]+$}}
    // CHECK-NEXT: [[BODY:bb[0-9]+]]:
    // CHECK-NEXT: JUMPDEST
    // CHECK: MUL
    // CHECK: JUMPI ; -> [[PANIC:bb[0-9]+]], else bb{{[0-9]+}}
    // CHECK-NOT: {{JUMP +; -> bb[0-9]+$}}
    // CHECK: JUMPDEST
    // CHECK-NOT: bb{{[0-9]+}}:
    // CHECK: JUMPI ; -> [[BODY]], else bb{{[0-9]+}}
    // CHECK: [[PANIC]]:
    // CHECK-NEXT: JUMPDEST
    // CHECK-NEXT: PUSH1 0x11
    function route(uint256 amount, uint256 hops) external pure returns (uint256) {
        for (uint256 i = 0; i < hops; ++i) {
            amount = amount * 997 / 1000;
        }
        return amount;
    }
}
