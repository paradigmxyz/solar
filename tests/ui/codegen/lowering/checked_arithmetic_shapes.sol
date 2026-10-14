//@ revisions: semantic expanded
//@[semantic] compile-flags: -O none -Zdump=mir
//@[expanded] compile-flags: -O none -Zdump=mir -Zmir-pipeline=lower-arithmetic
//@[semantic] filecheck: --check-prefix=SEM
//@[expanded] filecheck:

// Pins the per-op checked-arithmetic check shapes so they stay at or below
// solc's happy-path gas:
// - unsigned add/sub: single `lt` against an operand (sub-word: `gt` max).
// - unsigned mul: `or(iszero(rhs), eq(div(p, rhs), lhs))` (sub-word <= 128
//   bits: `gt` max only).
// - signed add/sub: `xor` of two `slt`/`sgt` comparisons, no constants.
// - signed mul: division-inverse check plus `and(eq(lhs, MIN), slt(rhs, 0))`.
// - div/mod: branch directly on the divisor, no `iszero`/`eq` flag.
// - sub-word left shifts: mask unsigned results and sign-extend signed results.
// SEM-LABEL: fn @sadd
// SEM: checked_add i256, arg0, arg1
// SEM-LABEL: fn @ssub
// SEM: checked_sub i256, arg0, arg1
// SEM-LABEL: fn @smul
// SEM: checked_mul i256, arg0, arg1
// SEM-LABEL: fn @sdiv
// SEM: checked_div i256, arg0, arg1
contract CheckedArithmeticShapes {
    // CHECK-LABEL: fn @sadd{{[( ]}}
    // CHECK: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK: slt arg0, 0
    // CHECK: slt arg1, 0
    // CHECK: slt [[SUM]], 0
    // CHECK: mstore 32, 17
    function sadd(int256 a, int256 b) public pure returns (int256) {
        return a + b;
    }

    // CHECK-LABEL: fn @ssub{{[( ]}}
    // CHECK: [[DIFF:v[0-9]+]] = sub arg0, arg1
    // CHECK: slt arg0, 0
    // CHECK: slt arg1, 0
    // CHECK: slt [[DIFF]], 0
    // CHECK: mstore 32, 17
    function ssub(int256 a, int256 b) public pure returns (int256) {
        return a - b;
    }

    // CHECK-LABEL: fn @smul{{[( ]}}
    // CHECK: [[PRODUCT:v[0-9]+]] = mul arg0, arg1
    // CHECK: sdiv [[PRODUCT]], arg1
    // CHECK-NOT: slt [[PRODUCT]],
    // CHECK-NOT: sgt [[PRODUCT]],
    // CHECK: eq arg0, 0x8000000000000000000000000000000000000000000000000000000000000000
    // CHECK-NEXT: {{v[0-9]+}} = slt arg1, 0
    // CHECK-NOT: slt [[PRODUCT]],
    // CHECK-NOT: sgt [[PRODUCT]],
    // CHECK: mstore 32, 17
    function smul(int256 a, int256 b) public pure returns (int256) {
        return a * b;
    }

    // CHECK-LABEL: fn @sdiv{{[( ]}}
    // CHECK: jumpi {{(arg[0-9]+|v[0-9]+)}},
    // CHECK: mstore 32, 18
    // CHECK: and {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: sdiv arg0, arg1
    function sdiv(int256 a, int256 b) public pure returns (int256) {
        return a / b;
    }

    // CHECK-LABEL: fn @smod{{[( ]}}
    // CHECK: jumpi {{(arg[0-9]+|v[0-9]+)}},
    // CHECK: mstore 32, 18
    // CHECK: smod arg0, arg1
    function smod(int256 a, int256 b) public pure returns (int256) {
        return a % b;
    }

    // CHECK-LABEL: fn @neg{{[( ]}}
    // CHECK: eq arg0, 0x8000000000000000000000000000000000000000000000000000000000000000
    // CHECK: sub 0, arg0
    // CHECK: mstore 32, 17
    function neg(int256 a) public pure returns (int256) {
        return -a;
    }

    // CHECK-LABEL: fn @inc{{[( ]}}
    // CHECK: [[RESULT:v[0-9]+]] = add arg0, 1
    // CHECK: lt [[RESULT]], arg0
    // CHECK: mstore 32, 17
    function inc(uint256 a) public pure returns (uint256) {
        return ++a;
    }

    // CHECK-LABEL: fn @dec{{[( ]}}
    // CHECK: [[RESULT:v[0-9]+]] = sub arg0, 1
    // CHECK: lt arg0, 1
    // CHECK: mstore 32, 17
    function dec(uint256 a) public pure returns (uint256) {
        return --a;
    }

    // CHECK-LABEL: fn @uadd128{{[( ]}}
    // CHECK: [[LHS:v[0-9]+]] = zext i128 arg0 to i256
    // CHECK: [[RHS:v[0-9]+]] = zext i128 arg1 to i256
    // CHECK: [[RESULT:v[0-9]+]] = add [[LHS]], [[RHS]]
    // CHECK: gt [[RESULT]], 0xffffffffffffffffffffffffffffffff
    // CHECK: mstore 32, 17
    function uadd128(uint128 a, uint128 b) public pure returns (uint128) {
        return a + b;
    }

    // CHECK-LABEL: fn @umul128{{[( ]}}
    // CHECK: [[LHS:v[0-9]+]] = zext i128 arg0 to i256
    // CHECK: [[RHS:v[0-9]+]] = zext i128 arg1 to i256
    // CHECK: [[RESULT:v[0-9]+]] = mul [[LHS]], [[RHS]]
    // CHECK: div [[RESULT]], [[RHS]]
    // CHECK: gt [[RESULT]], 0xffffffffffffffffffffffffffffffff
    // CHECK: mstore 32, 17
    function umul128(uint128 a, uint128 b) public pure returns (uint128) {
        return a * b;
    }

    // CHECK-LABEL: fn @smul128{{[( ]}}
    // CHECK: [[LHS:v[0-9]+]] = sext i128 arg0 to i256
    // CHECK: [[RHS:v[0-9]+]] = sext i128 arg1 to i256
    // CHECK: [[RESULT:v[0-9]+]] = mul [[LHS]], [[RHS]]
    // CHECK: sdiv [[RESULT]], [[RHS]]
    // CHECK: slt [[RESULT]], 0xffffffffffffffffffffffffffffffff80000000000000000000000000000000
    // CHECK: sgt [[RESULT]], 0x7fffffffffffffffffffffffffffffff
    // CHECK: mstore 32, 17
    function smul128(int128 a, int128 b) public pure returns (int128) {
        return a * b;
    }

    // CHECK-LABEL: fn @umul192{{[( ]}}
    // CHECK: [[LHS:v[0-9]+]] = zext i192 arg0 to i256
    // CHECK: [[RHS:v[0-9]+]] = zext i192 arg1 to i256
    // CHECK: [[RESULT:v[0-9]+]] = mul [[LHS]], [[RHS]]
    // CHECK: div [[RESULT]], [[RHS]]
    // CHECK: gt [[RESULT]], 0xffffffffffffffffffffffffffffffffffffffffffffffff
    // CHECK: mstore 32, 17
    function umul192(uint192 a, uint192 b) public pure returns (uint192) {
        return a * b;
    }

    // CHECK-LABEL: fn @leftU8(arg0: i8, arg1: i8) -> i8
    // CHECK: [[SHIFTED:v[0-9]+]] = shl arg1, arg0
    // CHECK-NEXT: ret [[SHIFTED]]
    function leftU8(uint8 value, uint8 bits) external pure returns (uint8) {
        return value << bits;
    }

    // CHECK-LABEL: fn @leftU16(arg0: i16, arg1: i8) -> i16
    // CHECK: [[BITS:v[0-9]+]] = zext i8 arg1 to i16
    // CHECK: [[SHIFTED:v[0-9]+]] = shl [[BITS]], arg0
    // CHECK-NEXT: ret [[SHIFTED]]
    function leftU16(uint16 value, uint8 bits) external pure returns (uint16) {
        return value << bits;
    }

    // CHECK-LABEL: fn @leftI8(arg0: i8, arg1: i8) -> i8
    // CHECK: [[SHIFTED:v[0-9]+]] = shl arg1, arg0
    // CHECK-NEXT: ret [[SHIFTED]]
    function leftI8(int8 value, uint8 bits) external pure returns (int8) {
        return value << bits;
    }

    // CHECK-LABEL: fn @leftI16(arg0: i16, arg1: i8) -> i16
    // CHECK: [[BITS:v[0-9]+]] = zext i8 arg1 to i16
    // CHECK: [[SHIFTED:v[0-9]+]] = shl [[BITS]], arg0
    // CHECK-NEXT: ret [[SHIFTED]]
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

    // CHECK-LABEL: fn @rightU8(arg0: i8, arg1: i8) -> i8
    // CHECK: [[SHIFTED:v[0-9]+]] = shr arg1, arg0
    // CHECK-NEXT: ret [[SHIFTED]]
    function rightU8(uint8 value, uint8 bits) external pure returns (uint8) {
        return value >> bits;
    }

    // CHECK-LABEL: fn @rightI8(arg0: i8, arg1: i8) -> i8
    // CHECK: [[SHIFTED:v[0-9]+]] = sar arg1, arg0
    // CHECK-NEXT: ret [[SHIFTED]]
    function rightI8(int8 value, uint8 bits) external pure returns (int8) {
        return value >> bits;
    }
}
