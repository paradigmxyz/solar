//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract C {
    // CHECK-LABEL: fn @owner
    // CHECK: {{v[0-9]+}} = loadimmutable 0
    address public immutable owner;

    // CHECK-LABEL: fn @_anonymous
    // CHECK: mstore 0x2000, arg0
    constructor(address value) {
        owner = value;
    }
}
