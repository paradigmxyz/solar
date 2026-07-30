//@ revisions: deploy runtime
//@[deploy] compile-flags: -Zcodegen -O none -Zdump=disasm-deploy
//@[deploy] filecheck: --check-prefix=DEPLOY --implicit-check-not=CALLDATALOAD
//@[runtime] compile-flags: -Zcodegen -O none -Zdump=disasm-runtime
//@[runtime] filecheck: --check-prefix=RUNTIME

contract C {
    // DEPLOY-LABEL: // === {{.*}}:C (deployment) ===
    // DEPLOY: CODECOPY
    // DEPLOY-NEXT: PUSH0
    // DEPLOY-NEXT: RETURN
    // DEPLOY-NOT: CALLDATALOAD

    // RUNTIME-LABEL: // === {{.*}}:C (runtime) ===
    // RUNTIME: CALLDATALOAD
    function f(uint256 x) external pure returns (uint256) {
        return x + 1;
    }
}
