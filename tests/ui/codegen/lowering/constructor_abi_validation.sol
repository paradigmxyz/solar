//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract ConstructorAbiValidation {
    // CHECK-LABEL: fn @flag{{[( ]}}
    // CHECK: trunc i256 {{v[0-9]+}} to i8
    bool public flag;

    // CHECK-LABEL: fn @second{{[( ]}}
    // CHECK: shr 8,
    // CHECK: trunc i256 {{v[0-9]+}} to i8
    bool public second;

    // CHECK-LABEL: fn @constructor{{[( ]}}
    // CHECK: zext i1 arg0 to i256
    // CHECK: [[FLAGS:v[0-9]+]] = make_memory_slice arg1, 2
    // CHECK: slice_load_word [[FLAGS]], 32
    // CHECK: sstore 0,
    constructor(bool flag_, bool[2] memory flags) {
        flag = flag_;
        second = flags[1];
    }
}
