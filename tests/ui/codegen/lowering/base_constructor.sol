//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract Base {
    // CHECK-LABEL: fn @value
    // CHECK: {{v[0-9]+}} = sload 0
    uint256 public value;

    // CHECK-LABEL: fn @_anonymous
    // CHECK: sstore 0, arg0
    constructor(uint256 initialValue) {
        value = initialValue;
    }
}

contract Derived is Base {
    // CHECK-LABEL: fn @_anonymous
    // CHECK: sstore 0, arg0
    // CHECK-LABEL: fn @value
    // CHECK: sload 0
    constructor(uint256 initialValue) Base(initialValue) {}
}
