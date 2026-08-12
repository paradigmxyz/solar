//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

// Pins the per-op checked-arithmetic check shapes so they stay at or below
// solc's happy-path gas:
// - unsigned add/sub: single `lt` against an operand (sub-word: `gt` max).
// - unsigned mul: `or(iszero(rhs), eq(div(p, rhs), lhs))` (sub-word <= 128
//   bits: `gt` max only).
// - signed add/sub: `xor` of two `slt`/`sgt` comparisons, no constants.
// - signed mul: division-inverse check plus `and(eq(lhs, MIN), slt(rhs, 0))`.
// - div/mod: branch directly on the divisor, no `iszero`/`eq` flag.
// - sub-word left shifts: mask unsigned results and sign-extend signed results.
contract CheckedArithmeticShapes {
    // CHECK-LABEL: fn @sadd{{[( ]}}
    // CHECK: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK: [[LHS_NEG:v[0-9]+]] = slt arg0, 0
    // CHECK: [[WRAPPED:v[0-9]+]] = slt [[SUM]], arg1
    // CHECK: xor [[LHS_NEG]], [[WRAPPED]]
    function sadd(int256 a, int256 b) public pure returns (int256) {
        return a + b;
    }

    // CHECK-LABEL: fn @ssub{{[( ]}}
    // CHECK: [[DIFF:v[0-9]+]] = sub arg0, arg1
    // CHECK: [[RHS_NEG:v[0-9]+]] = slt arg1, 0
    // CHECK: [[WRAPPED:v[0-9]+]] = sgt [[DIFF]], arg0
    // CHECK: xor [[RHS_NEG]], [[WRAPPED]]
    function ssub(int256 a, int256 b) public pure returns (int256) {
        return a - b;
    }

    // CHECK-LABEL: fn @smul{{[( ]}}
    // CHECK: [[PRODUCT:v[0-9]+]] = mul arg0, arg1
    // CHECK: sdiv [[PRODUCT]], arg1
    // CHECK: eq arg0, 0x8000000000000000000000000000000000000000000000000000000000000000
    // CHECK: slt arg1, 0
    function smul(int256 a, int256 b) public pure returns (int256) {
        return a * b;
    }

    // CHECK-LABEL: fn @sdiv{{[( ]}}
    // CHECK: jumpi arg1,
    // CHECK: mstore 4, 18
    // CHECK: and {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: sdiv arg0, arg1
    function sdiv(int256 a, int256 b) public pure returns (int256) {
        return a / b;
    }

    // CHECK-LABEL: fn @smod{{[( ]}}
    // CHECK: jumpi arg1,
    // CHECK: mstore 4, 18
    // CHECK: smod arg0, arg1
    function smod(int256 a, int256 b) public pure returns (int256) {
        return a % b;
    }

    // CHECK-LABEL: fn @neg{{[( ]}}
    // CHECK: eq arg0, 0x8000000000000000000000000000000000000000000000000000000000000000
    // CHECK: sub 0, arg0
    function neg(int256 a) public pure returns (int256) {
        return -a;
    }

    // CHECK-LABEL: fn @inc{{[( ]}}
    // CHECK: [[OLD:v[0-9]+]] = mload 128
    // CHECK: [[RESULT:v[0-9]+]] = add [[OLD]], 1
    // CHECK: lt [[RESULT]], [[OLD]]
    function inc(uint256 a) public pure returns (uint256) {
        return ++a;
    }

    // CHECK-LABEL: fn @dec{{[( ]}}
    // CHECK: [[OLD:v[0-9]+]] = mload 128
    // CHECK: [[RESULT:v[0-9]+]] = sub [[OLD]], 1
    // CHECK: lt [[OLD]], 1
    function dec(uint256 a) public pure returns (uint256) {
        return --a;
    }

    // CHECK-LABEL: fn @uadd128{{[( ]}}
    // CHECK: [[RESULT:v[0-9]+]] = add arg0, arg1
    // CHECK: gt [[RESULT]], 0xffffffffffffffffffffffffffffffff
    function uadd128(uint128 a, uint128 b) public pure returns (uint128) {
        return a + b;
    }

    // CHECK-LABEL: fn @umul128{{[( ]}}
    // CHECK: [[RESULT:v[0-9]+]] = mul arg0, arg1
    // CHECK: gt [[RESULT]], 0xffffffffffffffffffffffffffffffff
    function umul128(uint128 a, uint128 b) public pure returns (uint128) {
        return a * b;
    }

    // CHECK-LABEL: fn @smul128{{[( ]}}
    // CHECK: [[RESULT:v[0-9]+]] = mul arg0, arg1
    // CHECK: slt [[RESULT]], 0xffffffffffffffffffffffffffffffff80000000000000000000000000000000
    // CHECK: sgt [[RESULT]], 0x7fffffffffffffffffffffffffffffff
    function smul128(int128 a, int128 b) public pure returns (int128) {
        return a * b;
    }

    // CHECK-LABEL: fn @umul192{{[( ]}}
    // CHECK: [[RESULT:v[0-9]+]] = mul arg0, arg1
    // CHECK: div [[RESULT]], arg1
    // CHECK: gt [[RESULT]], 0xffffffffffffffffffffffffffffffffffffffffffffffff
    function umul192(uint192 a, uint192 b) public pure returns (uint192) {
        return a * b;
    }

    // CHECK-LABEL: fn @leftU8{{[( ]}}
    // CHECK: [[SHIFTED:v[0-9]+]] = shl arg1, arg0
    // CHECK-NEXT: [[CLEAN:v[0-9]+]] = and [[SHIFTED]], 255
    // CHECK-NEXT: ret [[CLEAN]]
    function leftU8(uint8 value, uint8 bits) external pure returns (uint8) {
        return value << bits;
    }

    // CHECK-LABEL: fn @leftU16{{[( ]}}
    // CHECK: [[SHIFTED:v[0-9]+]] = shl arg1, arg0
    // CHECK-NEXT: [[CLEAN:v[0-9]+]] = and [[SHIFTED]], 0xffff
    // CHECK-NEXT: ret [[CLEAN]]
    function leftU16(uint16 value, uint8 bits) external pure returns (uint16) {
        return value << bits;
    }

    // CHECK-LABEL: fn @leftI8{{[( ]}}
    // CHECK: [[SHIFTED:v[0-9]+]] = shl arg1, arg0
    // CHECK-NEXT: [[ALIGNED:v[0-9]+]] = shl 248, [[SHIFTED]]
    // CHECK-NEXT: [[CLEAN:v[0-9]+]] = sar 248, [[ALIGNED]]
    // CHECK-NEXT: ret [[CLEAN]]
    function leftI8(int8 value, uint8 bits) external pure returns (int8) {
        return value << bits;
    }

    // CHECK-LABEL: fn @leftI16{{[( ]}}
    // CHECK: [[SHIFTED:v[0-9]+]] = shl arg1, arg0
    // CHECK-NEXT: [[ALIGNED:v[0-9]+]] = shl 240, [[SHIFTED]]
    // CHECK-NEXT: [[CLEAN:v[0-9]+]] = sar 240, [[ALIGNED]]
    // CHECK-NEXT: ret [[CLEAN]]
    function leftI16(int16 value, uint8 bits) external pure returns (int16) {
        return value << bits;
    }

    // Full-width and right shifts already have native EVM word semantics.
    // CHECK-LABEL: fn @leftU256{{[( ]}}
    // CHECK: [[SHIFTED:v[0-9]+]] = shl arg1, arg0
    // CHECK-NEXT: ret [[SHIFTED]]
    function leftU256(uint256 value, uint256 bits) external pure returns (uint256) {
        return value << bits;
    }

    // CHECK-LABEL: fn @rightU8{{[( ]}}
    // CHECK: [[SHIFTED:v[0-9]+]] = shr arg1, arg0
    // CHECK-NEXT: ret [[SHIFTED]]
    function rightU8(uint8 value, uint8 bits) external pure returns (uint8) {
        return value >> bits;
    }

    // CHECK-LABEL: fn @rightI8{{[( ]}}
    // CHECK: [[SHIFTED:v[0-9]+]] = sar arg1, arg0
    // CHECK-NEXT: ret [[SHIFTED]]
    function rightI8(int8 value, uint8 bits) external pure returns (int8) {
        return value >> bits;
    }
}
