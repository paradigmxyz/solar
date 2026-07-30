//@ revisions: linear binary buckets dense perfect
//@[linear] compile-flags: -Zcodegen -O gas -Zswitch-lowering=linear -Zdump=disasm-runtime
//@[linear] filecheck: --check-prefix=LINEAR
//@[binary] compile-flags: -Zcodegen -O gas -Zswitch-lowering=binary -Zdump=disasm-runtime
//@[binary] filecheck: --check-prefix=BINARY
//@[buckets] compile-flags: -Zcodegen -O gas -Zswitch-lowering=buckets -Zdump=disasm-runtime
//@[buckets] filecheck: --check-prefix=BUCKETS
//@[dense] compile-flags: -Zcodegen -O gas -Zswitch-lowering=dense -Zdump=disasm-runtime
//@[dense] filecheck: --check-prefix=DENSE
//@[perfect] compile-flags: -Zcodegen -O gas -Zswitch-lowering=perfect -Zdump=disasm-runtime
//@[perfect] filecheck: --check-prefix=PERFECT

contract SwitchDisassembly {
    // LINEAR-LABEL: // === {{.*}}:SwitchDisassembly (runtime) ===
    // LINEAR: CALLDATALOAD
    // LINEAR: MSTORE
    // LINEAR-NEXT: DUP1
    // LINEAR-NEXT: PUSH1 0x08
    // LINEAR-NEXT: SUB
    // LINEAR: DUP1
    // LINEAR-NEXT: PUSH1 0x10
    // LINEAR-NEXT: SUB
    // LINEAR-NOT: GT
    // LINEAR-NOT: MOD

    // BINARY-LABEL: // === {{.*}}:SwitchDisassembly (runtime) ===
    // BINARY: CALLDATALOAD
    // BINARY: MSTORE
    // BINARY-NEXT: DUP1
    // BINARY-NEXT: PUSH1 0x28
    // BINARY-NEXT: GT
    // BINARY: PUSH1 0x18
    // BINARY-NEXT: GT

    // BUCKETS-LABEL: // === {{.*}}:SwitchDisassembly (runtime) ===
    // BUCKETS: CALLDATALOAD
    // BUCKETS: MSTORE
    // BUCKETS-NEXT: DUP1
    // BUCKETS-NEXT: PUSH1 0x09
    // BUCKETS-NEXT: SWAP1
    // BUCKETS-NEXT: MOD
    // BUCKETS: PUSH1 0x03
    // BUCKETS-NEXT: SHL
    // BUCKETS-NEXT: PUSH9 {{.*}}
    // BUCKETS-NEXT: SWAP1
    // BUCKETS-NEXT: SHR

    // DENSE-LABEL: // === {{.*}}:SwitchDisassembly (runtime) ===
    // DENSE: CALLDATALOAD
    // DENSE: PUSH1 0x08
    // DENSE-NEXT: SWAP1
    // DENSE-NEXT: SUB
    // DENSE: PUSH1 0x39
    // DENSE-NEXT: GT
    // DENSE: PUSH1 0x04
    // DENSE-NEXT: MUL
    // DENSE-NEXT: PUSH1 {{.*}}
    // DENSE-NEXT: ADD
    // DENSE-NEXT: JUMP

    // PERFECT-LABEL: // === {{.*}}:SwitchDisassembly (runtime) ===
    // PERFECT: CALLDATALOAD
    // PERFECT: MSTORE
    // PERFECT-NEXT: DUP1
    // PERFECT-NEXT: PUSH1 0x03
    // PERFECT-NEXT: SHR
    // PERFECT-NEXT: PUSH1 0x07
    // PERFECT-NEXT: AND
    // PERFECT: PUSH1 0x03
    // PERFECT-NEXT: SHL
    // PERFECT-NEXT: PUSH8 {{.*}}
    // PERFECT-NEXT: SWAP1
    // PERFECT-NEXT: SHR

    fallback() external {
        assembly {
            switch calldataload(0)
            case 8 {
                mstore(0, 100)
                return(0, 32)
            }
            case 16 {
                mstore(0, 101)
                return(0, 32)
            }
            case 24 {
                mstore(0, 102)
                return(0, 32)
            }
            case 32 {
                mstore(0, 103)
                return(0, 32)
            }
            case 40 {
                mstore(0, 104)
                return(0, 32)
            }
            case 48 {
                mstore(0, 105)
                return(0, 32)
            }
            case 56 {
                mstore(0, 106)
                return(0, 32)
            }
            case 64 {
                mstore(0, 107)
                return(0, 32)
            }
            default { revert(0, 0) }
        }
    }
}
