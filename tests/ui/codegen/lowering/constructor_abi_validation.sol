//@compile-flags: -Zcodegen -Zdump=mir
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
    // CHECK: mload 128
    // CHECK: revert 0, 0
    // CHECK: mload 160
    // CHECK: revert 0, 0
    // CHECK: mload 192
    // CHECK: revert 0, 0
    // CHECK: memory_object_element_addr memoryfixedarray<2, 1>, {{v[0-9]+}}, 1
    // CHECK: sstore 0,
    constructor(bool flag_, bool[2] memory flags) {
        flag = flag_;
        second = flags[1];
    }
}
