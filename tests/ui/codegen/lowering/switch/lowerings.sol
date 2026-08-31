//@ revisions: linear_gas linear_size binary_gas binary_size buckets_gas buckets_size dense_gas dense_size perfect_gas perfect_size

//@[linear_gas] compile-flags: -O gas -Zswitch-lowering=linear -Zdump=evm-ir-runtime,disasm-runtime
//@[linear_gas] filecheck: --check-prefix=LINEAR
//@[linear_size] compile-flags: -O size -Zswitch-lowering=linear -Zdump=evm-ir-runtime,disasm-runtime
//@[linear_size] filecheck: --check-prefix=LINEAR
//@[binary_gas] compile-flags: -O gas -Zswitch-lowering=binary -Zdump=evm-ir-runtime,disasm-runtime
//@[binary_gas] filecheck: --check-prefix=BINARY
//@[binary_size] compile-flags: -O size -Zswitch-lowering=binary -Zdump=evm-ir-runtime,disasm-runtime
//@[binary_size] filecheck: --check-prefix=BINARY
//@[buckets_gas] compile-flags: -O gas -Zswitch-lowering=buckets -Zdump=evm-ir-runtime,disasm-runtime
//@[buckets_gas] filecheck: --check-prefix=BUCKETSGAS
//@[buckets_size] compile-flags: -O size -Zswitch-lowering=buckets -Zdump=evm-ir-runtime,disasm-runtime
//@[buckets_size] filecheck: --check-prefix=BUCKETSSIZE
//@[dense_gas] compile-flags: -O gas -Zswitch-lowering=dense -Zdump=evm-ir-runtime,disasm-runtime
//@[dense_gas] filecheck: --check-prefix=DENSE
//@[dense_size] compile-flags: -O size -Zswitch-lowering=dense -Zdump=evm-ir-runtime,disasm-runtime
//@[dense_size] filecheck: --check-prefix=DENSE
//@[perfect_gas] compile-flags: -O gas -Zswitch-lowering=perfect -Zdump=evm-ir-runtime,disasm-runtime
//@[perfect_gas] filecheck: --check-prefix=PERFECTGAS
//@[perfect_size] compile-flags: -O size -Zswitch-lowering=perfect -Zdump=evm-ir-runtime,disasm-runtime
//@[perfect_size] filecheck: --check-prefix=PERFECTSIZE

contract SwitchLowerings {
    // LINEAR-LABEL: @module runtime
    // LINEAR: push 8
    // LINEAR-NEXT: sub
    // LINEAR-NEXT: push {{bb[0-9]+}}
    // LINEAR-NEXT: jumpi
    // LINEAR: push 16
    // LINEAR-NEXT: sub

    // BINARY-LABEL: @module runtime
    // BINARY: push 40
    // BINARY-NEXT: gt
    // BINARY-NEXT: push {{bb[0-9]+}}
    // BINARY-NEXT: jumpi

    // BUCKETSGAS-LABEL: @module runtime
    // BUCKETSGAS: push 9
    // BUCKETSGAS-NEXT: dup 2
    // BUCKETSGAS-NEXT: mod
    // BUCKETSGAS-NEXT: indexed_jump

    // BUCKETSSIZE-LABEL: @module runtime
    // BUCKETSSIZE: push 8
    // BUCKETSSIZE-NEXT: dup 2
    // BUCKETSSIZE-NEXT: mod
    // BUCKETSSIZE-NEXT: indexed_jump
    // BUCKETSSIZE: PUSH1 0x18
    // BUCKETSSIZE-NEXT: ADD
    // BUCKETSSIZE-NEXT: PUSH8
    // BUCKETSSIZE-NEXT: SWAP1
    // BUCKETSSIZE-NEXT: BYTE
    // BUCKETSSIZE-NEXT: JUMP ; unknown

    // DENSE-LABEL: @module runtime
    // DENSE: push 8
    // DENSE-NEXT: swap 1
    // DENSE-NEXT: sub
    // DENSE: push 57
    // DENSE-NEXT: gt
    // DENSE: indexed_jump

    // PERFECTGAS-LABEL: @module runtime
    // PERFECTGAS: push 3
    // PERFECTGAS-NEXT: shr
    // PERFECTGAS-NEXT: push 7
    // PERFECTGAS-NEXT: and
    // PERFECTGAS-NEXT: indexed_jump
    // PERFECTGAS: push 64
    // PERFECTGAS-NEXT: sub
    // PERFECTGAS-NEXT: push {{bb[0-9]+}}
    // PERFECTGAS-NEXT: jumpi

    // PERFECTSIZE-LABEL: @module runtime
    // PERFECTSIZE: push 8
    // PERFECTSIZE-NEXT: swap 1
    // PERFECTSIZE-NEXT: sub
    // PERFECTSIZE: push 3
    // PERFECTSIZE-NEXT: shr
    // PERFECTSIZE-NEXT: swap 1
    // PERFECTSIZE: push 253
    // PERFECTSIZE-NEXT: shl
    // PERFECTSIZE-NEXT: or
    // PERFECTSIZE: indexed_jump

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
