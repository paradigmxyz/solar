//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: assign 0x0000000000000000000000000000000000001234, 17 => 0x0000000000000000000000000000000000001234, 17
//@ run-call: assign 0xffffffffffffffffffffffffffffffffffffffff, 79228162514264337593543950335 => 0xffffffffffffffffffffffffffffffffffffffff, 79228162514264337593543950335
//@ run-call: assign 0x0000000000000000000000000000000000000000, 0 => 0x0000000000000000000000000000000000000000, 0

contract MaskBlendPackedStruct {
    struct Q {
        address a;
        uint96 b;
    }

    Q private q;

    // CHECK-LABEL: fn @assign
    // CHECK-NOT: sload
    // CHECK: sstore 0,
    // CHECK-NOT: sload
    // CHECK: ret
    function assign(address a, uint96 b) external returns (address, uint96) {
        q = Q(a, b);
        return (q.a, q.b);
    }
}
