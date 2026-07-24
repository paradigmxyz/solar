//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract MultiReturn {
    // CHECK-LABEL: fn @div_mod
    // CHECK: [[QUOTIENT:v[0-9]+]] = div arg0, arg1
    // CHECK: [[REMAINDER:v[0-9]+]] = mod arg0, arg1
    // CHECK: returndata 128, 64
    function div_mod(uint256 a, uint256 b) public pure returns (uint256, uint256) {
        return (a / b, a % b);
    }

    // CHECK-LABEL: fn @min_max
    // CHECK: [[ORDERED:v[0-9]+]] = lt arg0, arg1
    // CHECK: br [[ORDERED]],
    // CHECK-COUNT-2: returndata 128, 64
    function min_max(uint256 a, uint256 b) public pure returns (uint256, uint256) {
        if (a < b) {
            return (a, b);
        }
        return (b, a);
    }

    // CHECK-LABEL: fn @triple
    // CHECK: [[DOUBLE:v[0-9]+]] = add arg0, arg0
    // CHECK: [[TRIPLE:v[0-9]+]] = add {{v[0-9]+}}, arg0
    // CHECK: returndata 128, 96
    function triple(uint256 x) public pure returns (uint256, uint256, uint256) {
        return (x, x + x, x + x + x);
    }
}
