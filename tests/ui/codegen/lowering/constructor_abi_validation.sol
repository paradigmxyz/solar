//@compile-flags: -O none -Zdump=mir -Zmir-pipeline=lower-abi
//@filecheck:

contract ConstructorAbiValidation {
    // CHECK-LABEL: fn @flag{{[( ]}}
    // CHECK: and {{v[0-9]+}}, 255
    bool public flag;

    // CHECK-LABEL: fn @second{{[( ]}}
    // CHECK: shr 8,
    // CHECK: and {{v[0-9]+}}, 255
    bool public second;

    // CHECK-LABEL: fn @_anonymous(arg0: bool, arg1: bool, arg2: bool)
    // CHECK: lt arg0, 2
    // CHECK: memory_object_store_element memoryfixedarray<2, 1>, {{v[0-9]+}}, 1, arg2
    // CHECK: revert 0, 0
    constructor(bool flag_, bool[2] memory flags) {
        flag = flag_;
        second = flags[1];
    }
}
