//@ compile-flags: -Zcodegen -Zdump=mir
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
    // CHECK: and [[WORD]], 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00
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
    // CHECK: br [[A]],
    // CHECK: {{v[0-9]+}} = sload 0
    // CHECK: phi [bb0: 0],
    function both() external view returns (bool) {
        return a && b;
    }
}
