//@ revisions: capped allowed
//@[capped] compile-flags: -O gas --optimize-runs 1000000 -Zswitch-max-bit-slice-gas-code-growth=50 -Zdump=evm-ir-runtime
//@[capped] filecheck: --check-prefix=CAPPED
//@[allowed] compile-flags: -O gas --optimize-runs 1000000 -Zswitch-max-bit-slice-gas-code-growth=51 -Zdump=evm-ir-runtime
//@[allowed] filecheck: --check-prefix=ALLOWED

// Selector slots fall through to the next slot on a miss and share one
// default tail, so the model charges the bit-slice dispatch with 51 bytes
// of growth over a linear scan. A 50-byte cap rejects it and a 51-byte cap
// accepts it. Many expected calls make the table worth its deposit cost.
contract EntryBitSlice {
    // CAPPED-LABEL: @module EntryBitSlice_runtime
    // CAPPED: push 7{{$}}
    // CAPPED-NEXT: dup 2
    // CAPPED-NEXT: mod
    // CAPPED-NEXT: indexed_jump

    // ALLOWED-LABEL: @module EntryBitSlice_runtime
    // ALLOWED: push 22{{$}}
    // ALLOWED-NEXT: shr
    // ALLOWED-NEXT: push 7{{$}}
    // ALLOWED-NEXT: and
    // ALLOWED-NEXT: indexed_jump
    function f0() external {}
    function f1() external {}
    function f2() external {}
    function f3() external {}
    function f4() external {}
    function f5() external {}
    function f6() external {}
    function f7() external {}
}
