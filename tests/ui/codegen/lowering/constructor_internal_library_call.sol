//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

library ConstructorLibrary {
    // CHECK-LABEL: fn @helper
    // CHECK: [[NEXT:v[0-9]+]] = sub arg0, 1
    // CHECK: internal_call @helper, 1, [[NEXT]]
    function helper(uint256 n) internal pure returns (uint256) {
        if (n == 0) {
            return 1;
        }
        return n * 7 + helper(n - 1);
    }
}

contract ConstructorInternalLibraryCall {
    // CHECK-LABEL: fn @value
    // CHECK: sload 0
    uint256 public value;

    // CHECK-LABEL: fn @_anonymous
    // CHECK: [[MASKED:v[0-9]+]] = and arg0, 7
    // CHECK: [[VALUE:v[0-9]+]] = internal_call @helper, 1, [[MASKED]]
    // CHECK: sstore 0, [[VALUE]]
    // CHECK-LABEL: fn @helper
    // CHECK: internal_call @helper
    constructor(uint256 x) {
        value = ConstructorLibrary.helper(x & 7);
    }
}
