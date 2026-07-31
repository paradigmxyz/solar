//@ revisions: mir size
//@[mir] compile-flags: -Zcodegen -O none -Zdump=mir
//@[mir] filecheck:
//@[size] compile-flags: -Zcodegen -O size -Zdump=evm-ir-runtime

contract FunctionCall {
    // CHECK-LABEL: fn @double{{[( ]}}
    // CHECK: [[DOUBLE:v[0-9]+]] = add arg0, arg0
    // CHECK: ret [[DOUBLE]]
    function double(uint256 x) internal pure returns (uint256) {
        return x + x;
    }

    // CHECK-LABEL: fn @quadruple{{[( ]}}
    // CHECK: [[ONCE:v[0-9]+]] = internal_call @double, 1, arg0
    // CHECK: internal_call @double, 1, [[ONCE]]
    function quadruple(uint256 x) public pure returns (uint256) {
        return double(double(x));
    }

    // CHECK-LABEL: fn @sum_then_double{{[( ]}}
    // CHECK: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK: internal_call @double, 1, [[SUM]]
    function sum_then_double(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 s = a + b;
        return double(s);
    }
}
