//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract LoopSimple {
    // CHECK-LABEL: fn @sum_to{{[( ]}}
    // CHECK: [[I:v[0-9]+]] = mload 160
    // CHECK: lt [[I]], arg0
    // CHECK: {{v[0-9]+}} = mload 128
    // CHECK: ret
    // CHECK: [[LOOP_I:v[0-9]+]] = mload 160
    // CHECK: add [[LOOP_I]], 1
    // CHECK: [[TOTAL:v[0-9]+]] = mload 128
    // CHECK: [[BODY_I:v[0-9]+]] = mload 160
    // CHECK: add [[TOTAL]], [[BODY_I]]
    function sum_to(uint256 n) public pure returns (uint256) {
        uint256 total = 0;
        for (uint256 i = 0; i < n; i++) {
            total = total + i;
        }
        return total;
    }
}
