//@ revisions: ir disasm
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck: --check-prefix=IR
//@[disasm] compile-flags: -Ogas -Zdump=disasm-runtime
//@[disasm] filecheck: --check-prefix=DISASM --implicit-check-not=PUSH32

// Both revert blocks are cold, so each string takes its shifted push even in a gas build: the
// 15-byte string saves 14 bytes for 6 more gas, the 25-byte one only 4, which hot pricing would
// keep as a literal `PUSH32`.
contract ColdRevertStrings {
    // IR-LABEL: @module ColdRevertStrings_runtime
    // IR: [cold]:
    // IR: push 0x45524332303a503a494e56414c49445f5349474e4154555245
    // IR-NEXT: push 56
    // IR-NEXT: shl
    // IR: [cold]:
    // IR: push 0x45524332303a503a45585049524544
    // IR-NEXT: push 136
    // IR-NEXT: shl

    // DISASM-LABEL: // === {{.*}}:ColdRevertStrings (runtime) ===
    // DISASM: PUSH25 0x45524332303a503a494e56414c49445f5349474e4154555245
    // DISASM-NEXT: PUSH1 0x38
    // DISASM-NEXT: SHL
    // DISASM: PUSH15 0x45524332303a503a45585049524544
    // DISASM-NEXT: PUSH1 0x88
    // DISASM-NEXT: SHL
    function permit(uint256 deadline, bool valid) external view returns (uint256) {
        require(deadline >= block.timestamp, "ERC20:P:EXPIRED");
        require(valid, "ERC20:P:INVALID_SIGNATURE");
        return deadline;
    }
}
