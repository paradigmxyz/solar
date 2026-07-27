//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract Branch {
    // CHECK-LABEL: fn @max{{[( ]}}
    // CHECK: [[GT:v[0-9]+]] = gt arg0, arg1
    // CHECK: jumpi [[GT]],
    // CHECK: returndata
    // CHECK: returndata
    function max(uint256 a, uint256 b) public pure returns (uint256) {
        if (a > b) {
            return a;
        }
        return b;
    }

    // CHECK-LABEL: fn @abs_diff{{[( ]}}
    // CHECK: [[LT:v[0-9]+]] = lt arg0, arg1
    // CHECK: {{v[0-9]+}} = iszero [[LT]]
    // CHECK: {{v[0-9]+}} = sub arg0, arg1
    // CHECK: {{v[0-9]+}} = sub arg1, arg0
    function abs_diff(uint256 a, uint256 b) public pure returns (uint256) {
        if (a >= b) {
            return a - b;
        } else {
            return b - a;
        }
    }
}
