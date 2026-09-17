//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract MultiReturn {
    // CHECK-LABEL: fn @div_mod{{[( ]}}
    // CHECK: {{v[0-9]+}} = checked_div {{[ui][0-9]+}}, arg0, arg1
    // CHECK: {{v[0-9]+}} = checked_rem {{[ui][0-9]+}}, arg0, arg1
    // CHECK: [[RET_0:v[0-9]+]] = insert_value [[RET_TY:struct[0-9]+]], undef [[RET_TY]], 0, {{v[0-9]+}}
    // CHECK: [[RET_1:v[0-9]+]] = insert_value [[RET_TY]], [[RET_0]], 1, {{v[0-9]+}}
    // CHECK: ret [[RET_1]]
    function div_mod(uint256 a, uint256 b) public pure returns (uint256, uint256) {
        return (a / b, a % b);
    }

    // CHECK-LABEL: fn @min_max{{[( ]}}
    // CHECK: [[ORDERED:v[0-9]+]] = lt arg0, arg1
    // CHECK: jumpi [[ORDERED]],
    // CHECK-COUNT-2: ret
    function min_max(uint256 a, uint256 b) public pure returns (uint256, uint256) {
        if (a < b) {
            return (a, b);
        }
        return (b, a);
    }

    // CHECK-LABEL: fn @triple{{[( ]}}
    // CHECK: {{v[0-9]+}} = checked_add {{[ui][0-9]+}}, arg0, arg0
    // CHECK: {{v[0-9]+}} = checked_add {{[ui][0-9]+}}, {{v[0-9]+}}, arg0
    // CHECK: [[RET_0:v[0-9]+]] = insert_value [[RET_TY:struct[0-9]+]], undef [[RET_TY]], 0, arg0
    // CHECK: [[RET_1:v[0-9]+]] = insert_value [[RET_TY]], [[RET_0]], 1, {{v[0-9]+}}
    // CHECK: [[RET_2:v[0-9]+]] = insert_value [[RET_TY]], [[RET_1]], 2, {{v[0-9]+}}
    // CHECK: ret [[RET_2]]
    function triple(uint256 x) public pure returns (uint256, uint256, uint256) {
        return (x, x + x, x + x + x);
    }
}
