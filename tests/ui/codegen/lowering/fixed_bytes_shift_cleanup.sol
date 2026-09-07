//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=evm-ir-runtime
//@[gas] filecheck:
//@ run-call: aligned 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xff, 0xffff, 0xffffffff
//@ run-call: aligned 0x123456 => 0x56, 0x3456, 0x00123456
//@ run-call: narrower 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xff
//@ run-call: narrower 0x1234 => 0x12
// CHECK-LABEL: @module FixedBytesShiftCleanup_runtime
contract FixedBytesShiftCleanup {
    // CHECK: push 240
    // CHECK-NEXT: shl
    // CHECK-NOT: and
    // CHECK: return
    function aligned(uint256 dirty) external pure returns (bytes1 a, bytes2 b, bytes4 c) {
        assembly {
            a := shl(248, dirty)
            b := shl(240, dirty)
            c := shl(224, dirty)
        }
    }

    // CHECK: push 240
    // CHECK-NEXT: shl
    // CHECK: and
    // CHECK: return
    function narrower(uint256 dirty) external pure returns (bytes1 result) {
        assembly { result := shl(240, dirty) }
    }
}
