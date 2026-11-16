//@ revisions: perfect_gas perfect_size buckets_gas buckets_size
//@[perfect_gas] compile-flags: -O gas -Zswitch-lowering=perfect -Zdump=evm-ir-runtime
//@[perfect_gas] filecheck: --check-prefix=PERFECT
//@[perfect_size] compile-flags: -O size -Zswitch-lowering=perfect -Zdump=evm-ir-runtime
//@[perfect_size] filecheck: --check-prefix=PERFECT
//@[buckets_gas] compile-flags: -O gas -Zswitch-lowering=buckets
//@[buckets_size] compile-flags: -O size -Zswitch-lowering=buckets -Zdump=evm-ir-runtime
//@[buckets_size] filecheck: --check-prefix=BUCKETS
//@ run-call: select 0 => 1
//@ run-call: select 0x8000000000000000 => 2
//@ run-call: select 0x10000000000000000 => 3
//@ run-call: select 0x18000000000000001 => 4
//@ run-call: select 1 => 999
//@ run-call: select 0x8000000000000001 => 999

// The keys differ only in bits 0, 63, and 64, so table indices must read
// across the first 64-bit limb of each key.
contract WideKeys {
    // Bits 63 and 64 form the only collision-free two-bit slice.
    // PERFECT-LABEL: @module WideKeys_runtime
    // PERFECT: push 63{{$}}
    // PERFECT-NEXT: shr
    // PERFECT-NEXT: push 3{{$}}
    // PERFECT-NEXT: and
    // PERFECT-NEXT: indexed_jump

    // Three buckets sort keys by their full value modulo 3.
    // BUCKETS-LABEL: @module WideKeys_runtime
    // BUCKETS: push 3{{$}}
    // BUCKETS-NEXT: dup 2
    // BUCKETS-NEXT: mod
    // BUCKETS-NEXT: indexed_jump
    function select(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 0 { result := 1 }
            case 0x8000000000000000 { result := 2 }
            case 0x10000000000000000 { result := 3 }
            case 0x18000000000000001 { result := 4 }
            default { result := 999 }
        }
    }
}
