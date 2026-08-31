//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract ConstructorAbiValidation {
    // CHECK-LABEL: fn @flag{{[( ]}}
    // CHECK: and {{v[0-9]+}}, 255
    bool public flag;

    // CHECK-LABEL: fn @second{{[( ]}}
    // CHECK: shr 8,
    // CHECK: and {{v[0-9]+}}, 255
    bool public second;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: and arg0, 255
    // CHECK: memory_object_load_element memoryfixedarray<2, 1>, arg1, 1
    // CHECK: sstore 0,
    constructor(bool flag_, bool[2] memory flags) {
        flag = flag_;
        second = flags[1];
    }
}
