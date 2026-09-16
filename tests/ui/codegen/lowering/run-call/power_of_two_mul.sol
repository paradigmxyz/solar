//@ codegen-matrix: standard irgas irsize legacy
//@[irgas] compile-flags: -Ogas --evm-version=constantinople -Zdump=evm-ir-runtime
//@[irsize] compile-flags: -Osize --evm-version=constantinople -Zdump=evm-ir-runtime
//@[legacy] compile-flags: -Ogas --evm-version=byzantium -Zdump=evm-ir-runtime
//@[irgas] filecheck: --check-prefix=MODERN --implicit-check-not={{^[[:blank:]]*exp$}}
//@[irsize] filecheck: --check-prefix=MODERN --implicit-check-not={{^[[:blank:]]*exp$}}
//@[legacy] filecheck: --check-prefix=LEGACY --implicit-check-not={{^[[:blank:]]*(shl|shr|sar)$}}
// MODERN-LABEL: @module PowerOfTwoMul_runtime
// MODERN: {{^ *}}shl{{$}}
// LEGACY-LABEL: @module PowerOfTwoMul_runtime
// LEGACY: {{^ *}}push 4{{$}}
// LEGACY-NEXT: calldataload
// LEGACY-NEXT: push 2
// LEGACY-NEXT: exp
//@[none] compile-flags: --evm-version=byzantium
//@ run-call: mul32 0 => 0
//@ run-call: mul32 1 => 32
//@ run-call: mul32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
//@ run-call: mul32 0x8000000000000000000000000000000000000000000000000000000000000000 => 0
//@ run-call: mulHigh 3 => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: mulHigh 2 => 0
//@ run-call: at [11,22,33], 0 => 11
//@ run-call: at [11,22,33], 2 => 33
//@ run-call-fail: at [11,22,33], 3 => Panic(0x32)

//@ run-call: exp2 0 => 1
//@ run-call: exp2 1 => 2
//@ run-call: exp2 255 => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: exp2 256 => 0
//@ run-call: exp2 257 => 0
//@ run-call: exp2 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: uncheckedExp2 255 => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: uncheckedExp2 256 => 0
//@ run-call: checkedExp2 255 => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: checkedExp2 256 => Panic(0x11)

contract PowerOfTwoMul {
    function exp2(uint256 exponent) external pure returns (uint256 result) {
        assembly { result := exp(2, exponent) }
    }

    function uncheckedExp2(uint256 exponent) external pure returns (uint256) {
        unchecked { return 2 ** exponent; }
    }

    function checkedExp2(uint256 exponent) external pure returns (uint256) {
        return 2 ** exponent;
    }

    function mul32(uint256 value) external pure returns (uint256 result) {
        assembly { result := mul(value, 32) }
    }

    function mulHigh(uint256 value) external pure returns (uint256 result) {
        assembly {
            result := mul(0x8000000000000000000000000000000000000000000000000000000000000000, value)
        }
    }

    function at(uint256[] memory values, uint256 index) external pure returns (uint256) {
        return values[index];
    }
}
