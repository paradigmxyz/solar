//@compile-flags: -O none -Zdump=mir -Zmir-pipeline=lower-arithmetic
//@filecheck:

// Pins checked exponentiation lowering:
// - checked paths use exponentiation by squaring and guard only products that
//   contribute to the result;
// - signed and sub-word products use their type bounds, and products of at most
//   128-bit operands only those;
// - a constant nonnegative base is one `EXP` after a bound on the exponent, the
//   largest whose power fits the type, while a negative one keeps the loop;
// - unchecked paths keep native `EXP` and truncate narrow results.
contract CheckedPowShapes {
    // CHECK-LABEL: fn @upow{{[( ]}}
    // CHECK-NOT: exp arg0, arg1
    // CHECK: phi [bb0: 1]
    // CHECK: and {{v[0-9]+}}, 1
    // CHECK: mul {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: shr {{[^ ,]+}}, 1{{$}}
    // CHECK: mstore 32, 17
    function upow(uint256 a, uint256 b) public pure returns (uint256) {
        return a ** b;
    }

    // CHECK-LABEL: fn @spow{{[( ]}}
    // CHECK: phi [bb0: 1]
    // CHECK: and {{v[0-9]+}}, 1
    // CHECK: sdiv {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK-NOT: slt
    // CHECK-NOT: sgt
    // CHECK: eq {{v[0-9]+}}, 0x8000000000000000000000000000000000000000000000000000000000000000
    // CHECK: mul {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: mstore 32, 17
    function spow(int256 a, uint256 b) public pure returns (int256) {
        return a ** b;
    }

    // CHECK-LABEL: fn @upow8{{[( ]}}
    // CHECK: phi [bb0: 1]
    // CHECK-NOT: div
    // CHECK: gt {{v[0-9]+}}, 255
    // CHECK: mstore 32, 17
    function upow8(uint8 a, uint8 b) public pure returns (uint8) {
        return a ** b;
    }

    // CHECK-LABEL: fn @spow8{{[( ]}}
    // CHECK: phi [bb0: 1]
    // CHECK: slt {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80
    // CHECK: sgt {{v[0-9]+}}, 127
    // CHECK: mstore 32, 17
    function spow8(int8 a, uint8 b) public pure returns (int8) {
        return a ** b;
    }

    // CHECK-LABEL: fn @const2{{[( ]}}
    // CHECK-NOT: phi
    // CHECK: [[OVER:v[0-9]+]] = gt arg0, 255
    // CHECK: jumpi [[OVER]]
    // CHECK: exp 2, arg0
    // CHECK: mstore 32, 17
    function const2(uint256 b) public pure returns (uint256) {
        return 2 ** b;
    }

    // CHECK-LABEL: fn @const10{{[( ]}}
    // CHECK-NOT: phi
    // CHECK: gt arg0, 77
    // CHECK: exp 10, arg0
    // CHECK: mstore 32, 17
    function const10(uint256 b) public pure returns (uint256) {
        return 10 ** b;
    }

    // CHECK-LABEL: fn @const10_8{{[( ]}}
    // CHECK-NOT: phi
    // CHECK: gt {{v[0-9]+}}, 2
    // CHECK: exp 10, {{v[0-9]+}}
    // CHECK: mstore 32, 17
    function const10_8(uint8 b) public pure returns (uint8) {
        return uint8(10) ** b;
    }

    // CHECK-LABEL: fn @const3_signed{{[( ]}}
    // CHECK-NOT: phi
    // CHECK: gt arg0, 160
    // CHECK: exp 3, arg0
    // CHECK: mstore 32, 17
    function const3_signed(uint256 b) public pure returns (int256) {
        return int256(3) ** b;
    }

    // CHECK-LABEL: fn @const1{{[( ]}}
    // CHECK-NOT: phi
    // CHECK-NOT: gt
    // CHECK: exp 1, arg0
    // CHECK-NOT: mstore 32, 17
    function const1(uint256 b) public pure returns (uint256) {
        return 1 ** b;
    }

    // CHECK-LABEL: fn @const_neg2{{[( ]}}
    // CHECK: phi [bb0: 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe]
    // CHECK: sdiv {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: mstore 32, 17
    function const_neg2(uint256 b) public pure returns (int256) {
        return (-2) ** b;
    }

    // CHECK-LABEL: fn @unchecked_pow8{{[( ]}}
    // CHECK-NOT: mstore 32, 17
    // CHECK: [[BASE:v[0-9]+]] = zext i8 arg0 to i256
    // CHECK: [[EXPONENT:v[0-9]+]] = zext i8 arg1 to i256
    // CHECK: [[POWER:v[0-9]+]] = exp [[BASE]], [[EXPONENT]]
    // CHECK: trunc i256 [[POWER]] to i8
    // CHECK-NOT: mstore 32, 17
    function unchecked_pow8(uint8 a, uint8 b) public pure returns (uint8) {
        unchecked {
            return a ** b;
        }
    }
}
