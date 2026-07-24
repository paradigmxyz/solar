//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract FunctionCall {
    // CHECK-LABEL: fn @double
    // CHECK: [[DOUBLE:v[0-9]+]] = add arg0, arg0
    // CHECK: ret [[DOUBLE]]
    function double(uint256 x) internal pure returns (uint256) {
        return x + x;
    }

    // CHECK-LABEL: fn @quadruple
    // CHECK: [[DOUBLE:v[0-9]+]] = add arg0, arg0
    // CHECK: [[QUADRUPLE:v[0-9]+]] = add [[DOUBLE]], [[DOUBLE]]
    function quadruple(uint256 x) public pure returns (uint256) {
        return double(double(x));
    }

    // CHECK-LABEL: fn @sum_then_double
    // CHECK: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK: [[DOUBLE:v[0-9]+]] = add [[SUM]], [[SUM]]
    function sum_then_double(uint256 a, uint256 b) public pure returns (uint256) {
        uint256 s = a + b;
        return double(s);
    }
}
