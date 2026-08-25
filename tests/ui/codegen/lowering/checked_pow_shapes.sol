//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Pins checked exponentiation lowering:
// - checked paths use exponentiation by squaring and guard only products that
//   contribute to the result;
// - signed and sub-word products use their type bounds;
// - unchecked paths keep native `EXP` and truncate narrow results.
contract CheckedPowShapes {
    // CHECK-LABEL: fn @upow{{[( ]}}
    // CHECK-NOT: exp arg0, arg1
    // CHECK: phi [bb0: 1]
    // CHECK: and {{v[0-9]+}}, 1
    // CHECK: mul {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: shr 1,
    // CHECK: mstore 4, 17
    function upow(uint256 a, uint256 b) public pure returns (uint256) {
        return a ** b;
    }

    // CHECK-LABEL: fn @spow{{[( ]}}
    // CHECK: phi [bb0: 1]
    // CHECK: and {{v[0-9]+}}, 1
    // CHECK: sdiv {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: slt {{v[0-9]+}}, 0x8000000000000000000000000000000000000000000000000000000000000000
    // CHECK: mul {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: mstore 4, 17
    function spow(int256 a, uint256 b) public pure returns (int256) {
        return a ** b;
    }

    // CHECK-LABEL: fn @upow8{{[( ]}}
    // CHECK: phi [bb0: 1]
    // CHECK: gt {{v[0-9]+}}, 255
    // CHECK: mstore 4, 17
    function upow8(uint8 a, uint8 b) public pure returns (uint8) {
        return a ** b;
    }

    // CHECK-LABEL: fn @spow8{{[( ]}}
    // CHECK: phi [bb0: 1]
    // CHECK: slt {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80
    // CHECK: sgt {{v[0-9]+}}, 127
    // CHECK: mstore 4, 17
    function spow8(int8 a, uint8 b) public pure returns (int8) {
        return a ** b;
    }

    // CHECK-LABEL: fn @const2{{[( ]}}
    // CHECK: phi [bb0: 1]
    // CHECK: phi [bb0: 2]
    // CHECK: mstore 4, 17
    function const2(uint256 b) public pure returns (uint256) {
        return 2 ** b;
    }

    // CHECK-LABEL: fn @const10{{[( ]}}
    // CHECK: phi [bb0: 10]
    // CHECK: mstore 4, 17
    function const10(uint256 b) public pure returns (uint256) {
        return 10 ** b;
    }

    // CHECK-LABEL: fn @const_neg2{{[( ]}}
    // CHECK: [[BASE:v[0-9]+]] = sub 0, 2
    // CHECK: phi [bb0: [[BASE]]]
    // CHECK: sdiv {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: mstore 4, 17
    function const_neg2(uint256 b) public pure returns (int256) {
        return (-2) ** b;
    }

    // CHECK-LABEL: fn @unchecked_pow8{{[( ]}}
    // CHECK-NOT: mstore 4, 17
    // CHECK: [[POWER:v[0-9]+]] = exp arg0, arg1
    // CHECK: and [[POWER]], 255
    // CHECK-NOT: mstore 4, 17
    function unchecked_pow8(uint8 a, uint8 b) public pure returns (uint8) {
        unchecked {
            return a ** b;
        }
    }
}
