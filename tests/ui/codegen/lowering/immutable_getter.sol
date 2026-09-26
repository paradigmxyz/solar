//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract C {
    // CHECK-LABEL: fn @owner{{[( ]}}
    // CHECK: {{v[0-9]+}} = loadimmutable owner
    address public immutable owner;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: [[OWNER:v[0-9]+]] = zext i160 arg0 to i256
    // CHECK: storeimmutable owner, [[OWNER]]
    constructor(address value) {
        owner = value;
    }
}
