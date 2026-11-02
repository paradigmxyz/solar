//@ compile-flags: -O none -Zdump=mir
//@ filecheck:

contract PackedBool {
    // CHECK-LABEL: fn @a{{[( ]}}
    // CHECK: [[WORD:v[0-9]+]] = sload 0
    // CHECK: trunc i256 [[WORD]] to i8
    bool public a;

    // CHECK-LABEL: fn @b{{[( ]}}
    // CHECK: [[WORD:v[0-9]+]] = sload 0
    // CHECK: [[SHIFTED:v[0-9]+]] = shr [[WORD]], 8
    // CHECK: trunc i256 [[SHIFTED]] to i8
    bool public b;

    // CHECK-LABEL: fn @set{{[( ]}}
    // CHECK: [[WORD:v[0-9]+]] = sload 0
    // CHECK: and [[WORD]], 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00
    // CHECK: sstore 0,
    // CHECK: {{v[0-9]+}} = sload 0
    // CHECK: shl {{[^ ,]+}}, 8{{$}}
    // CHECK: sstore 0,
    function set(bool x, bool y) external {
        a = x;
        b = y;
    }

    // CHECK-LABEL: fn @both{{[( ]}}
    // CHECK: [[WORD:v[0-9]+]] = sload 0
    // CHECK: [[A:v[0-9]+]] = trunc i256 [[WORD]] to i8
    // CHECK: [[COND:v[0-9]+]] = ne [[A]], i8 0
    // CHECK: jumpi [[COND]],
    // CHECK: {{v[0-9]+}} = sload 0
    // CHECK: {{v[0-9]+}} = shr {{[^ ,]+}}, 8{{$}}
    // CHECK: {{v[0-9]+}} = trunc i256 {{v[0-9]+}} to i8
    // CHECK: phi
    function both() external view returns (bool) {
        return a && b;
    }
}
