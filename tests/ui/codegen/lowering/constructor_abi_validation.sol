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
    // CHECK: [[END:v[0-9]+]] = fmp
    // CHECK-NEXT: [[BASE:v[0-9]+]] = constructor_args_base
    // CHECK-NEXT: [[SIZE:v[0-9]+]] = sub [[END]], [[BASE]]
    // CHECK-NEXT: [[SHORT:v[0-9]+]] = lt [[SIZE]], 96
    // CHECK-NEXT: jumpi [[SHORT]],
    // CHECK: set_fmp
    // CHECK: [[FIRST:v[0-9]+]] = add [[BASE]], 0
    // CHECK: [[FIRST_END:v[0-9]+]] = add [[FIRST]], 32
    // CHECK: gt [[FIRST_END]], [[END]]
    // CHECK: {{v[0-9]+}} = mload [[FIRST]]
    // CHECK: revert 0, 0
    // CHECK: [[SECOND:v[0-9]+]] = add [[BASE]], 32
    // CHECK: [[SECOND_END:v[0-9]+]] = add [[SECOND]], 32
    // CHECK: gt [[SECOND_END]], [[END]]
    // CHECK: {{v[0-9]+}} = mload [[SECOND]]
    // CHECK: revert 0, 0
    // CHECK: [[THIRD:v[0-9]+]] = add [[BASE]], 64
    // CHECK: [[THIRD_END:v[0-9]+]] = add [[THIRD]], 32
    // CHECK: gt [[THIRD_END]], [[END]]
    // CHECK: {{v[0-9]+}} = mload [[THIRD]]
    // CHECK: revert 0, 0
    // CHECK: memory_object_element_addr memoryfixedarray<2, 1>, {{v[0-9]+}}, 1
    // CHECK: sstore 0,
    constructor(bool flag_, bool[2] memory flags) {
        flag = flag_;
        second = flags[1];
    }
}
