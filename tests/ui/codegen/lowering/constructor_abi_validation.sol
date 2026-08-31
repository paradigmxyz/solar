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

    // CHECK-LABEL: fn @_anonymous(arg0: u256, arg1: u256, arg2: u256)
    // CHECK: [[BASE:v[0-9]+]] = constructor_args_base
    // CHECK: [[FLAG:v[0-9]+]] = mload [[BASE]]
    // CHECK: lt [[FLAG]], 2
    // CHECK: [[INDEX:v[0-9]+]] = phi
    // CHECK: [[ELEMENT:v[0-9]+]] = mload
    // CHECK: lt [[ELEMENT]], 2
    // CHECK: memory_object_store_element memoryfixedarray<2, 1>, {{v[0-9]+}}, [[INDEX]], [[ELEMENT]]
    // CHECK: revert 0, 0
    constructor(bool flag_, bool[2] memory flags) {
        flag = flag_;
        second = flags[1];
    }
}
