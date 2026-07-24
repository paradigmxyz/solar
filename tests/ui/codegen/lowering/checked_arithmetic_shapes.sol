//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

// Pins the per-op checked-arithmetic check shapes so they stay at or below
// solc's happy-path gas:
// - unsigned add/sub: single `lt` against an operand (sub-word: `gt` max).
// - unsigned mul: `or(iszero(rhs), eq(div(p, rhs), lhs))` (sub-word <= 128
//   bits: `gt` max only).
// - signed add/sub: `xor` of two `slt`/`sgt` comparisons, no constants.
// - signed mul: division-inverse check plus `and(eq(lhs, MIN), slt(rhs, 0))`.
// - div/mod: branch directly on the divisor, no `iszero`/`eq` flag.
contract CheckedArithmeticShapes {
    // CHECK-LABEL: fn @sadd
    // CHECK: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK: [[LHS_NEG:v[0-9]+]] = slt arg0, 0
    // CHECK: [[WRAPPED:v[0-9]+]] = slt [[SUM]], arg1
    // CHECK: xor [[LHS_NEG]], [[WRAPPED]]
    function sadd(int256 a, int256 b) public pure returns (int256) {
        return a + b;
    }

    // CHECK-LABEL: fn @ssub
    // CHECK: [[DIFF:v[0-9]+]] = sub arg0, arg1
    // CHECK: [[RHS_NEG:v[0-9]+]] = slt arg1, 0
    // CHECK: [[WRAPPED:v[0-9]+]] = sgt [[DIFF]], arg0
    // CHECK: xor [[RHS_NEG]], [[WRAPPED]]
    function ssub(int256 a, int256 b) public pure returns (int256) {
        return a - b;
    }

    // CHECK-LABEL: fn @smul
    // CHECK: [[PRODUCT:v[0-9]+]] = mul arg0, arg1
    // CHECK: sdiv [[PRODUCT]], arg1
    // CHECK: eq arg0, 0x8000000000000000000000000000000000000000000000000000000000000000
    // CHECK: slt arg1, 0
    function smul(int256 a, int256 b) public pure returns (int256) {
        return a * b;
    }

    // CHECK-LABEL: fn @sdiv
    // CHECK: br arg1,
    // CHECK: mstore 4, 18
    // CHECK: and {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: sdiv arg0, arg1
    function sdiv(int256 a, int256 b) public pure returns (int256) {
        return a / b;
    }

    // CHECK-LABEL: fn @smod
    // CHECK: br arg1,
    // CHECK: mstore 4, 18
    // CHECK: smod arg0, arg1
    function smod(int256 a, int256 b) public pure returns (int256) {
        return a % b;
    }

    // CHECK-LABEL: fn @neg
    // CHECK: eq arg0, 0x8000000000000000000000000000000000000000000000000000000000000000
    // CHECK: sub 0, arg0
    function neg(int256 a) public pure returns (int256) {
        return -a;
    }

    // CHECK-LABEL: fn @inc
    // CHECK: [[RESULT:v[0-9]+]] = add arg0, 1
    // CHECK: lt [[RESULT]], arg0
    function inc(uint256 a) public pure returns (uint256) {
        return ++a;
    }

    // CHECK-LABEL: fn @dec
    // CHECK: [[RESULT:v[0-9]+]] = sub arg0, 1
    // CHECK: lt arg0, 1
    function dec(uint256 a) public pure returns (uint256) {
        return --a;
    }

    // CHECK-LABEL: fn @uadd128
    // CHECK: [[RESULT:v[0-9]+]] = add arg0, arg1
    // CHECK: gt [[RESULT]], 0xffffffffffffffffffffffffffffffff
    function uadd128(uint128 a, uint128 b) public pure returns (uint128) {
        return a + b;
    }

    // CHECK-LABEL: fn @umul128
    // CHECK: [[RESULT:v[0-9]+]] = mul arg0, arg1
    // CHECK: gt [[RESULT]], 0xffffffffffffffffffffffffffffffff
    function umul128(uint128 a, uint128 b) public pure returns (uint128) {
        return a * b;
    }

    // CHECK-LABEL: fn @smul128
    // CHECK: [[RESULT:v[0-9]+]] = mul arg0, arg1
    // CHECK: slt [[RESULT]], 0xffffffffffffffffffffffffffffffff80000000000000000000000000000000
    // CHECK: sgt [[RESULT]], 0x7fffffffffffffffffffffffffffffff
    function smul128(int128 a, int128 b) public pure returns (int128) {
        return a * b;
    }

    // CHECK-LABEL: fn @umul192
    // CHECK: [[RESULT:v[0-9]+]] = mul arg0, arg1
    // CHECK: div [[RESULT]], arg1
    // CHECK: gt [[RESULT]], 0xffffffffffffffffffffffffffffffffffffffffffffffff
    function umul192(uint192 a, uint192 b) public pure returns (uint192) {
        return a * b;
    }
}
