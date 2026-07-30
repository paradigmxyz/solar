//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract ConstructorAbiValidation {
    // CHECK-LABEL: fn @flag{{[( ]}}
    // CHECK: and {{v[0-9]+}}, 255
    bool public flag;

    // CHECK-LABEL: fn @second{{[( ]}}
    // CHECK: shr 8,
    // CHECK: and {{v[0-9]+}}, 255
    bool public second;

    // CHECK-LABEL: fn @_anonymous{{[( ]}}
    // CHECK: [[BASE:v[0-9]+]] = constructor_args_base
    // CHECK: [[FIRST:v[0-9]+]] = add [[BASE]], 0
    // CHECK-NEXT: {{v[0-9]+}} = mload [[FIRST]]
    // CHECK: revert 0, 0
    // CHECK: [[SECOND:v[0-9]+]] = add [[BASE]], 32
    // CHECK-NEXT: {{v[0-9]+}} = mload [[SECOND]]
    // CHECK: revert 0, 0
    // CHECK: [[THIRD:v[0-9]+]] = add [[BASE]], 64
    // CHECK-NEXT: {{v[0-9]+}} = mload [[THIRD]]
    // CHECK: revert 0, 0
    // CHECK: memory_object_element_addr memoryfixedarray<2, 1>, {{v[0-9]+}}, 1
    // CHECK: sstore 0,
    constructor(bool flag_, bool[2] memory flags) {
        flag = flag_;
        second = flags[1];
    }
}
