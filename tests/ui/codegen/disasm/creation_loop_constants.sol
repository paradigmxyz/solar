//@ codegen-matrix: standard deploy
//@[deploy] compile-flags: -Ogas -Zdump=disasm-deploy
//@[deploy] filecheck:
//@ run-call: arr 5; constructor=[1] => 18

// The deployment pays for every byte once, but a loop body runs once per iteration, so the
// loop keeps the gas-first overflow check against a 32-byte limit that a straight-line
// constructor trades for a division.
// CHECK-LABEL: // === {{.*}}:Looped (deployment) ===
// CHECK: PUSH32 0x5555555555555555555555555555555555555555555555555555555555555555
contract Looped {
    uint256[100] public arr;

    constructor(uint256 s) {
        for (uint256 i; i < 100; ++i) {
            arr[i] = (s + i) * 3;
        }
    }
}
