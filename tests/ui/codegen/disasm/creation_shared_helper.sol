//@ codegen-matrix: standard deploy runtime
//@[deploy] compile-flags: -Ogas -Zdump=disasm-deploy
//@[deploy] filecheck: --check-prefix=DEPLOY
//@[runtime] compile-flags: -Ogas -Zdump=disasm-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME
//@ run-call: v; constructor=[5] => 15
//@ run-call: f 2; constructor=[5] => 15

// A helper that the runtime also calls is not creation-only code, so the constructor's copy
// of it and the runtime's both keep the gas-first overflow check against a 32-byte limit.
// DEPLOY-LABEL: // === {{.*}}:Shared (deployment) ===
// DEPLOY: PUSH32 0x5555555555555555555555555555555555555555555555555555555555555555
// RUNTIME-LABEL: // === {{.*}}:Shared (runtime) ===
// RUNTIME: PUSH32 0x5555555555555555555555555555555555555555555555555555555555555555
contract Shared {
    uint256 public v;

    constructor(uint256 p) {
        v = triple(p);
    }

    function triple(uint256 p) internal pure returns (uint256) {
        return p * 3;
    }

    function f(uint256 p) external pure returns (uint256) {
        return triple(p) + triple(p + 1);
    }
}
