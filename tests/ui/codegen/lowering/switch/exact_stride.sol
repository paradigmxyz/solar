//@ revisions: perfect_size auto_size legacy
//@[perfect_size] compile-flags: -Osize -Zswitch-lowering=perfect -Zdump=evm-ir-runtime
//@[perfect_size] filecheck: --check-prefixes=CHECK,FORCED
//@[auto_size] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[auto_size] filecheck:
//@[legacy] compile-flags: -Osize --evm-version=byzantium -Zswitch-lowering=perfect -Zdump=evm-ir-runtime
//@[legacy] filecheck: --check-prefix=LEGACY
//@ run-call: ExactStrideSwitch::choose 0 => 999
//@ run-call: ExactStrideSwitch::choose 7 => 999
//@ run-call: ExactStrideSwitch::choose 8 => 100
//@ run-call: ExactStrideSwitch::choose 9 => 999
//@ run-call: ExactStrideSwitch::choose 16 => 101
//@ run-call: ExactStrideSwitch::choose 64 => 107
//@ run-call: ExactStrideSwitch::choose 65 => 999
//@ run-call: ExactStrideSwitch::choose 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 999
//@ run-call: WrappedStrideSwitch::choose 0 => 999
//@ run-call: WrappedStrideSwitch::choose 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe2 => 999
//@ run-call: WrappedStrideSwitch::choose 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe3 => 200
//@ run-call: WrappedStrideSwitch::choose 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe4 => 999
//@ run-call: WrappedStrideSwitch::choose 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe7 => 201
//@ run-call: WrappedStrideSwitch::choose 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 999
//@ run-call: WrappedStrideSwitch::choose 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 207
//@ run-call: LargeStrideSwitch::choose 0 => 999
//@ run-call: LargeStrideSwitch::choose 1 => 300
//@ run-call: LargeStrideSwitch::choose 2 => 999
//@ run-call: LargeStrideSwitch::choose 0x8000000000000000000000000000000000000000000000000000000000000000 => 999
//@ run-call: LargeStrideSwitch::choose 0x8000000000000000000000000000000000000000000000000000000000000001 => 301
//@ run-call: LargeStrideSwitch::choose 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 999
//@ run-call: FallbackStrideSwitch::choose 8 => 400
//@ run-call: FallbackStrideSwitch::choose 9 => 999
//@ run-call: FallbackStrideSwitch::choose 11 => 401
//@ run-call: FallbackStrideSwitch::choose 20 => 404
//@ run-call: FallbackStrideSwitch::choose 21 => 999
//@ run-call: FallbackStrideSwitch::choose 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 999
//@ run-call: FallbackStrideSwitch::single 0 => 999
//@ run-call: FallbackStrideSwitch::single 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 999
//@ run-call: FallbackStrideSwitch::single 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 77

// A below-minimum value wraps during normalization; rotation must retain invalid low bits.
contract ExactStrideSwitch {
    // CHECK-LABEL: @module ExactStrideSwitch_runtime
    // CHECK: push 253
    // CHECK-NEXT: shl
    // CHECK-NEXT: or
    // CHECK: indexed_jump
    // LEGACY-LABEL: @module ExactStrideSwitch_runtime
    // LEGACY-NOT: indexed_jump
    function choose(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 8 { result := 100 }
            case 16 { result := 101 }
            case 24 { result := 102 }
            case 32 { result := 103 }
            case 40 { result := 104 }
            case 48 { result := 105 }
            case 56 { result := 106 }
            case 64 { result := 107 }
            default { result := 999 }
        }
    }
}

contract WrappedStrideSwitch {
    // CHECK-LABEL: @module WrappedStrideSwitch_runtime
    // CHECK: push 254
    // CHECK-NEXT: shl
    // CHECK-NEXT: or
    // CHECK: indexed_jump
    function choose(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe3 { result := 200 }
            case 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe7 { result := 201 }
            case 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffeb { result := 202 }
            case 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffef { result := 203 }
            case 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff3 { result := 204 }
            case 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7 { result := 205 }
            case 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb { result := 206 }
            case 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff { result := 207 }
            default { result := 999 }
        }
    }
}

contract LargeStrideSwitch {
    // FORCED-LABEL: @module LargeStrideSwitch_runtime
    // FORCED: push 1
    // FORCED-NEXT: shl
    // FORCED-NEXT: or
    // FORCED: indexed_jump
    function choose(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 1 { result := 300 }
            case 0x8000000000000000000000000000000000000000000000000000000000000001 { result := 301 }
            default { result := 999 }
        }
    }
}

contract FallbackStrideSwitch {
    function choose(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 8 { result := 400 }
            case 11 { result := 401 }
            case 14 { result := 402 }
            case 17 { result := 403 }
            case 20 { result := 404 }
            default { result := 999 }
        }
    }

    function single(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff { result := 77 }
            default { result := 999 }
        }
    }
}
