//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract Linear {
    // CHECK-LABEL: fn @add
    // CHECK: [[RESULT:v[0-9]+]] = add arg0, arg1
    // CHECK: lt [[RESULT]], arg0
    function add(uint256 x, uint256 y) public pure returns (uint256) {
        return x + y;
    }

    // CHECK-LABEL: fn @sub
    // CHECK: [[RESULT:v[0-9]+]] = sub arg0, arg1
    // CHECK: lt arg0, arg1
    function sub(uint256 x, uint256 y) public pure returns (uint256) {
        return x - y;
    }

    // CHECK-LABEL: fn @add_one
    // CHECK: [[RESULT:v[0-9]+]] = add arg0, 1
    // CHECK: lt [[RESULT]], arg0
    function add_one(uint256 x) public pure returns (uint256) {
        return x + 1;
    }
}
