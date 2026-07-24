//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract NestedLoops {
    // CHECK-LABEL: fn @sum_grid
    // CHECK: [[I:v[0-9]+]] = mload 192
    // CHECK: lt [[I]], arg0
    // CHECK: [[J:v[0-9]+]] = mload 224
    // CHECK: lt [[J]], arg1
    // CHECK: [[BODY_I:v[0-9]+]] = mload 192
    // CHECK: [[BODY_J:v[0-9]+]] = mload 224
    // CHECK: {{v[0-9]+}} = mul [[BODY_I]], [[BODY_J]]
    function sum_grid(uint256 n, uint256 m) public pure returns (uint256) {
        uint256 total = 0;
        for (uint256 i = 0; i < n; i++) {
            for (uint256 j = 0; j < m; j++) {
                total = total + i * j;
            }
        }
        return total;
    }

    // CHECK-LABEL: fn @find_first
    // CHECK: [[I:v[0-9]+]] = mload 160
    // CHECK: lt [[I]], arg0
    // CHECK: [[J:v[0-9]+]] = mload 192
    // CHECK: lt [[J]], arg0
    // CHECK: add {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: eq {{v[0-9]+}}, arg1
    function find_first(uint256 n, uint256 target) public pure returns (uint256) {
        for (uint256 i = 0; i < n; i++) {
            for (uint256 j = 0; j < n; j++) {
                if (i + j == target) {
                    return i;
                }
            }
        }
        return n;
    }
}
