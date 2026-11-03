//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Operands are canonical as soon as an instruction is built: constants on the right, a compare of
// two values always `lt`, and the more complex operand of a commutative operation on the left.
contract CanonicalOperands {
    // CHECK-LABEL: fn @constantRight{{[( ]}}
    // CHECK: add arg0, 5
    function constantRight(uint256 x) public pure returns (uint256) {
        unchecked {
            return 5 + x;
        }
    }

    // CHECK-LABEL: fn @constantCompare{{[( ]}}
    // CHECK: gt arg0, 5
    function constantCompare(uint256 x) public pure returns (bool) {
        return 5 < x;
    }

    // CHECK-LABEL: fn @valueCompare{{[( ]}}
    // CHECK: lt arg1, arg0
    // CHECK-NOT: gt
    function valueCompare(uint256 a, uint256 b) public pure returns (bool) {
        return a > b;
    }

    // CHECK-LABEL: fn @signedValueCompare{{[( ]}}
    // CHECK: slt arg1, arg0
    // CHECK-NOT: sgt
    function signedValueCompare(int256 a, int256 b) public pure returns (bool) {
        return a > b;
    }

    // CHECK-LABEL: fn @complexLeft{{[( ]}}
    // CHECK: [[PRODUCT:v[0-9]+]] = mul arg0, arg1
    // CHECK: xor [[PRODUCT]], arg2
    function complexLeft(uint256 a, uint256 b, uint256 c) public pure returns (uint256) {
        unchecked {
            return c ^ (a * b);
        }
    }
}
