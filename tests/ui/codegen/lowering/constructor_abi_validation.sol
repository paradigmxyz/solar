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
    // CHECK: [[SIZE:v[0-9]+]] = mload 32 !metadata(memory=scratch)
    // CHECK: [[SHORT:v[0-9]+]] = lt [[SIZE]], 96
    // CHECK: jumpi [[SHORT]], [[SHORT_FAIL:bb[0-9]+]],
    // CHECK: [[SHORT_FAIL]]:
    // CHECK-NEXT: revert 0, 0
    // CHECK: [[FLAG:v[0-9]+]] = mload 128
    // CHECK: [[FLAG_ZERO:v[0-9]+]] = iszero [[FLAG]]
    // CHECK: [[FLAG_CLEAN:v[0-9]+]] = iszero [[FLAG_ZERO]]
    // CHECK: [[FLAG_VALID:v[0-9]+]] = eq [[FLAG]], [[FLAG_CLEAN]]
    // CHECK: jumpi [[FLAG_VALID]], {{bb[0-9]+}}, [[FLAG_FAIL:bb[0-9]+]]
    // CHECK: [[FLAG_FAIL]]:
    // CHECK-NEXT: revert 0, 0
    // CHECK: [[ARRAY:v[0-9]+]] = add 128, 32
    // CHECK: [[FIRST:v[0-9]+]] = mload [[ARRAY]]
    // CHECK: [[FIRST_ZERO:v[0-9]+]] = iszero [[FIRST]]
    // CHECK: [[FIRST_CLEAN:v[0-9]+]] = iszero [[FIRST_ZERO]]
    // CHECK: [[FIRST_VALID:v[0-9]+]] = eq [[FIRST]], [[FIRST_CLEAN]]
    // CHECK: jumpi [[FIRST_VALID]], {{bb[0-9]+}}, [[FIRST_FAIL:bb[0-9]+]]
    // CHECK: [[FIRST_FAIL]]:
    // CHECK-NEXT: revert 0, 0
    // CHECK: [[SECOND_PTR:v[0-9]+]] = add [[ARRAY]], 32
    // CHECK: [[SECOND:v[0-9]+]] = mload [[SECOND_PTR]]
    // CHECK: [[SECOND_ZERO:v[0-9]+]] = iszero [[SECOND]]
    // CHECK: [[SECOND_CLEAN:v[0-9]+]] = iszero [[SECOND_ZERO]]
    // CHECK: [[SECOND_VALID:v[0-9]+]] = eq [[SECOND]], [[SECOND_CLEAN]]
    // CHECK: jumpi [[SECOND_VALID]], {{bb[0-9]+}}, [[SECOND_FAIL:bb[0-9]+]]
    // CHECK: [[SECOND_FAIL]]:
    // CHECK-NEXT: revert 0, 0
    // CHECK: memory_object_element_addr memoryfixedarray<2, 1>, {{v[0-9]+}}, 1
    // CHECK: sstore 0,
    constructor(bool flag_, bool[2] memory flags) {
        flag = flag_;
        second = flags[1];
    }
}
