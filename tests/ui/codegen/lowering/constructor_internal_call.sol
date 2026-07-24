//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract ConstructorInternalCall {
    // CHECK-LABEL: fn @value
    // CHECK: sload 0
    uint256 public value;

    // CHECK-LABEL: fn @_anonymous
    // CHECK: [[MASKED:v[0-9]+]] = and arg0, 7
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @helper, 1, [[MASKED]]
    // CHECK: sstore 0, [[VALUE]]
    constructor(uint256 x) {
        value = helper(x & 7);
    }

    // CHECK-LABEL: fn @helper
    // CHECK: [[NEXT:v[0-9]+]] = sub arg0, 1
    // CHECK: [[RECURSED:v[0-9]+]] = internal_call @helper, 1, [[NEXT]]
    // CHECK: ret
    function helper(uint256 n) internal pure returns (uint256) {
        if (n == 0) {
            return 1;
        }
        return n * 11 + helper(n - 1);
    }
}
