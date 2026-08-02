//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract Ternary {
    // CHECK-LABEL: fn @max{{[( ]}}
    // CHECK: [[GT:v[0-9]+]] = gt arg0, arg1
    // CHECK: jumpi [[GT]],
    // CHECK: frame_store scratch, word, {{[0-9]+}}, arg0
    // CHECK: frame_store scratch, word, {{[0-9]+}}, arg1
    // CHECK: frame_load scratch, word, {{[0-9]+}}
    function max(uint256 a, uint256 b) public pure returns (uint256) {
        return a > b ? a : b;
    }

    // CHECK-LABEL: fn @clamp{{[( ]}}
    // CHECK: {{v[0-9]+}} = lt arg0, arg1
    // CHECK: {{v[0-9]+}} = gt arg0, arg2
    // CHECK: frame_load scratch, word, {{[0-9]+}}
    function clamp(uint256 x, uint256 lo, uint256 hi) public pure returns (uint256) {
        return x < lo ? lo : (x > hi ? hi : x);
    }

    // CHECK-LABEL: fn @abs_diff{{[( ]}}
    // CHECK: [[LT:v[0-9]+]] = lt arg0, arg1
    // CHECK: {{v[0-9]+}} = iszero [[LT]]
    // CHECK: sub arg0, arg1
    // CHECK: sub arg1, arg0
    // CHECK: frame_load scratch, word, {{[0-9]+}}
    function abs_diff(uint256 a, uint256 b) public pure returns (uint256) {
        return a >= b ? a - b : b - a;
    }
}
