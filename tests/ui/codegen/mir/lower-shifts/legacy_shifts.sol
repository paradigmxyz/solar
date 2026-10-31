//@ codegen-matrix: standard ir
//@ compile-flags: --evm-version byzantium
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck: --implicit-check-not={{^[[:blank:]]*(shl|shr|sar)$}} --implicit-check-not={{^[[:blank:]]*swap[[:blank:]]+1$}}
//@ run-call: shr8 => 0
//@ run-call: shl8 => 1024
//@ run-call: shl3 => 32
//@ run-call: selectorShift => 0

// Before Constantinople, a constant shift of a word that is still to be produced becomes a
// `mul` or `div` by a pushed `2**shift`. The scheduler pushes the factor first, so neither the
// selector extraction nor the shifts below need a `swap 1` to put the dividend on top.
// CHECK-LABEL: @module LegacyShifts_runtime
// CHECK: push 0x10000000000000000000000000000
// CHECK-NEXT: dup 1
// CHECK-NEXT: mul
// CHECK-NEXT: returndatasize
// CHECK-NEXT: calldataload
// CHECK-NEXT: div
// CHECK: push 8
// CHECK-NEXT: calldatasize
// CHECK-NEXT: mul
// CHECK: push 256
// CHECK-NEXT: calldatasize
// CHECK-NEXT: div
// CHECK: push 256
// CHECK-NEXT: calldatasize
// CHECK-NEXT: mul
contract LegacyShifts {
    function shr8() external pure returns (uint256) {
        return msg.data.length >> 8;
    }

    function shl8() external pure returns (uint256) {
        return msg.data.length << 8;
    }

    function shl3() external pure returns (uint256) {
        return msg.data.length << 3;
    }

    function selectorShift() external pure returns (uint256) {
        return msg.data.length >> 224;
    }
}
