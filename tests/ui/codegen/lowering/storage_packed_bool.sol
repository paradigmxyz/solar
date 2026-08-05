//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck:

contract PackedBool {
    // CHECK-LABEL: fn @a{{[( ]}}
    // CHECK: [[WORD:v[0-9]+]] = sload 0
    // CHECK: and [[WORD]], 255
    bool public a;

    // CHECK-LABEL: fn @b{{[( ]}}
    // CHECK: [[WORD:v[0-9]+]] = sload 0
    // CHECK: [[SHIFTED:v[0-9]+]] = shr 8, [[WORD]]
    // CHECK: and [[SHIFTED]], 255
    bool public b;

    // CHECK-LABEL: fn @set{{[( ]}}
    // CHECK: [[WORD:v[0-9]+]] = sload 0
    // CHECK: {{v[0-9]+}} = not 255
    // CHECK: and [[WORD]], {{v[0-9]+}}
    // CHECK: sstore 0,
    // CHECK: {{v[0-9]+}} = sload 0
    // CHECK: shl 8,
    // CHECK: sstore 0,
    function set(bool x, bool y) external {
        a = x;
        b = y;
    }

    // CHECK-LABEL: fn @both{{[( ]}}
    // CHECK: [[WORD:v[0-9]+]] = sload 0
    // CHECK: [[A:v[0-9]+]] = and [[WORD]], 255
    // CHECK: jumpi [[A]],
    // CHECK: {{v[0-9]+}} = sload 0
    // CHECK: {{v[0-9]+}} = shr 8,
    // CHECK: {{v[0-9]+}} = and {{v[0-9]+}}, 255
    // CHECK: phi
    function both() external view returns (bool) {
        return a && b;
    }
}
