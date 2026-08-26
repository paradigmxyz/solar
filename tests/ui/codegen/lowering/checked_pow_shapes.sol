//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Pins the checked exponentiation shapes ported from solc's `checked_exp_*`
// Yul helpers:
// - unsigned: trivial bases (0, 1) and base 2 short-circuit; bases below
//   11/307 with exponents below 78/32 use native `EXP` (exact, no wrap);
//   everything else runs the squaring loop with one `base * base` overflow
//   check per iteration and a checked final multiply.
// - signed: exponent 0/1 and bases 0/1/-1 short-circuit; the first squaring
//   is pulled out (only iteration with a possibly negative base); positive
//   bases reuse the small-base native `EXP` path with a range check; the
//   final multiply is bounds-checked per sign.
// - constant full-width bases compile to a single exponent bound check plus
//   native `EXP` (`SHL` for base 2).
// - unchecked exponentiation stays native `EXP` (masked for sub-word types).
contract CheckedPowShapes {
    // CHECK-LABEL: fn @upow{{[( ]}}
    // CHECK-DAG: eq arg0, 2
    // CHECK-DAG: shl arg1, 1
    // CHECK-DAG: exp arg0, arg1
    // CHECK-DAG: {{v[0-9]+}} = mul {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK-DAG: shr 1,
    function upow(uint256 a, uint256 b) public pure returns (uint256) {
        return a ** b;
    }

    // CHECK-LABEL: fn @spow{{[( ]}}
    // CHECK-DAG: eq arg0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
    // CHECK-DAG: exp arg0, arg1
    // CHECK-DAG: sdiv 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, arg0
    // CHECK-DAG: mul {{v[0-9]+}}, {{v[0-9]+}}
    function spow(int256 a, uint256 b) public pure returns (int256) {
        return a ** b;
    }

    // CHECK-LABEL: fn @upow8{{[( ]}}
    // CHECK-DAG: shl arg1, 1
    // CHECK-DAG: gt {{v[0-9]+}}, 255
    // CHECK-DAG: exp arg0, arg1
    // CHECK-DAG: gt {{v[0-9]+}}, 255
    function upow8(uint8 a, uint8 b) public pure returns (uint8) {
        return a ** b;
    }

    // CHECK-LABEL: fn @spow8{{[( ]}}
    // CHECK-DAG: exp arg0, arg1
    // CHECK-DAG: gt {{v[0-9]+}}, 127
    // CHECK-DAG: sdiv 127, arg0
    // CHECK-DAG: sdiv 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80,
    function spow8(int8 a, uint8 b) public pure returns (int8) {
        return a ** b;
    }

    // CHECK-LABEL: fn @const2{{[( ]}}
    // CHECK: gt arg0, 255
    // CHECK: shl arg0, 1
    function const2(uint256 b) public pure returns (uint256) {
        return 2 ** b;
    }

    // CHECK-LABEL: fn @const10{{[( ]}}
    // CHECK: gt arg0, 77
    // CHECK: exp 10, arg0
    function const10(uint256 b) public pure returns (uint256) {
        return 10 ** b;
    }

    // CHECK-LABEL: fn @const_neg2{{[( ]}}
    // CHECK: exp 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, arg0
    function const_neg2(uint256 b) public pure returns (int256) {
        return (-2) ** b;
    }

    // CHECK-LABEL: fn @unchecked_pow8{{[( ]}}
    // CHECK-NOT: mstore 4, 17
    // CHECK: [[POWER:v[0-9]+]] = exp arg0, arg1
    // CHECK-NOT: mstore 4, 17
    // CHECK: and [[POWER]], 255
    // CHECK-NOT: mstore 4, 17
    function unchecked_pow8(uint8 a, uint8 b) public pure returns (uint8) {
        unchecked {
            return a ** b;
        }
    }
}
