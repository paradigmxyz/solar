//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract InternalVoidCall {
    // CHECK-LABEL: fn @value
    // CHECK: sload 0
    uint256 public value;

    // CHECK-LABEL: fn @set
    // CHECK: [[ZERO:v[0-9]+]] = eq arg0, 0
    // CHECK: {{v[0-9]+}} = iszero [[ZERO]]
    // CHECK: sstore 0, arg0
    function set(uint256 newValue) public {
        writeIfNonZero(newValue);
    }

    // CHECK-LABEL: fn @setUnlessZero
    // CHECK: [[ZERO:v[0-9]+]] = eq arg0, 0
    // CHECK: br [[ZERO]],
    // CHECK: sstore 0, arg0
    function setUnlessZero(uint256 newValue) public {
        if (newValue == 0) {
            return;
        }
        value = newValue;
    }

    // CHECK-LABEL: fn @writeIfNonZero
    // CHECK: [[ZERO:v[0-9]+]] = eq arg0, 0
    // CHECK: {{v[0-9]+}} = iszero [[ZERO]]
    // CHECK: sstore 0, arg0
    function writeIfNonZero(uint256 newValue) internal {
        if (newValue != 0) {
            value = newValue;
        }
    }
}
