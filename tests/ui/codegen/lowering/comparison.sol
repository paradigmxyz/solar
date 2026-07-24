//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract Comparison {
    // CHECK-LABEL: fn @eq
    // CHECK: {{v[0-9]+}} = eq arg0, arg1
    function eq(uint256 a, uint256 b) public pure returns (bool) {
        return a == b;
    }

    // CHECK-LABEL: fn @lt
    // CHECK: {{v[0-9]+}} = lt arg0, arg1
    function lt(uint256 a, uint256 b) public pure returns (bool) {
        return a < b;
    }

    // CHECK-LABEL: fn @is_zero
    // CHECK: {{v[0-9]+}} = eq arg0, 0
    function is_zero(uint256 a) public pure returns (bool) {
        return a == 0;
    }
}
