//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract CheckedArithmeticPanic {
    // CHECK-LABEL: fn @add{{[( ]}}
    // CHECK: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK: lt [[SUM]], arg0
    // CHECK: tail_call @__panic_11
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    // CHECK-LABEL: fn @sub{{[( ]}}
    // CHECK: {{v[0-9]+}} = sub arg0, arg1
    // CHECK: lt arg0, arg1
    // CHECK: tail_call @__panic_11
    function sub(uint256 a, uint256 b) public pure returns (uint256) {
        return a - b;
    }

    // CHECK-LABEL: fn @mul{{[( ]}}
    // CHECK: [[PRODUCT:v[0-9]+]] = mul arg0, arg1
    // CHECK: iszero arg1
    // CHECK: div [[PRODUCT]], arg1
    // CHECK: tail_call @__panic_11
    function mul(uint256 a, uint256 b) public pure returns (uint256) {
        return a * b;
    }

    // CHECK-LABEL: fn @div_zero{{[( ]}}
    // CHECK: jumpi arg1,
    // CHECK: tail_call @__panic_12
    // CHECK: div arg0, arg1
    function div_zero(uint256 a, uint256 b) public pure returns (uint256) {
        return a / b;
    }

    // CHECK-LABEL: fn @pow{{[( ]}}
    // CHECK: iszero arg1
    // CHECK: shl arg1, 1
    // CHECK: exp arg0, arg1
    // CHECK: tail_call @__panic_11
    function pow(uint256 a, uint256 b) public pure returns (uint256) {
        return a ** b;
    }

    // CHECK-LABEL: fn @signed_add{{[( ]}}
    // CHECK: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK: slt arg0, 0
    // CHECK: slt [[SUM]], arg1
    // CHECK: xor
    function signed_add(int256 a, int256 b) public pure returns (int256) {
        return a + b;
    }

    // CHECK-LABEL: fn @signed_neg{{[( ]}}
    // CHECK: eq arg0, 0x8000000000000000000000000000000000000000000000000000000000000000
    // CHECK: tail_call @__panic_11
    // CHECK: sub 0, arg0
    function signed_neg(int256 a) public pure returns (int256) {
        return -a;
    }

    // CHECK-LABEL: fn @narrow_add{{[( ]}}
    // CHECK: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK: gt [[SUM]], 255
    // CHECK: tail_call @__panic_11
    function narrow_add(uint8 a, uint8 b) public pure returns (uint8) {
        return a + b;
    }

    // CHECK-LABEL: fn @unchecked_add{{[( ]}}
    // CHECK: add arg0, arg1
    // CHECK-NOT: mstore 4, 17
    // CHECK: ret
    function unchecked_add(uint256 a, uint256 b) public pure returns (uint256) {
        unchecked {
            return a + b;
        }
    }

    // CHECK-LABEL: fn @unchecked_neg{{[( ]}}
    // CHECK-NOT: mstore 4, 17
    // CHECK: sub 0, arg0
    // CHECK-NOT: mstore 4, 17
    // CHECK: ret
    // CHECK-NOT: mstore 4, 17
    function unchecked_neg(int256 a) public pure returns (int256) {
        unchecked {
            return -a;
        }
    }

    // CHECK-LABEL: fn @unchecked_pow{{[( ]}}
    // CHECK-NOT: mstore 4, 17
    // CHECK: exp arg0, arg1
    // CHECK-NOT: mstore 4, 17
    // CHECK: ret
    // CHECK-NOT: mstore 4, 17
    function unchecked_pow(uint256 a, uint256 b) public pure returns (uint256) {
        unchecked {
            return a ** b;
        }
    }

    // CHECK-LABEL: fn @unchecked_call{{[( ]}}
    // CHECK: internal_call @checked_inner, 1, arg0, arg1
    function unchecked_call(uint256 a, uint256 b) public pure returns (uint256) {
        unchecked {
            return checked_inner(a, b);
        }
    }

    // CHECK-LABEL: fn @checked_inner{{[( ]}}
    // CHECK: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK: lt [[SUM]], arg0
    // CHECK: ret [[SUM]]
    function checked_inner(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }
}
