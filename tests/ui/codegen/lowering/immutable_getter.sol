//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract C {
    // CHECK-LABEL: fn @owner{{[( ]}}
    // CHECK: {{v[0-9]+}} = loadimmutable owner
    address public immutable owner;

    // CHECK-LABEL: fn @_anonymous{{[( ]}}
    // CHECK: storeimmutable owner, arg0
    constructor(address value) {
        owner = value;
    }
}
