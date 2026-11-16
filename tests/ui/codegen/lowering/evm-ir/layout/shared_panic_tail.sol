//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=evm-ir-runtime
//@[gas] filecheck:
//@[size] compile-flags: -Zdump=evm-ir-runtime
//@[size] filecheck:
//@ run-call: add 1, 2 => 3
//@ run-call-fail: add 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: div 7, 2 => 3
//@ run-call-fail: div 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call: at 2 => 3
//@ run-call-fail: at 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A panic stores its code before the selector, so panics with different codes
// differ only in their first push and share one tail that stores the selector
// and reverts.
// CHECK-LABEL: @module SharedPanicTail_runtime
// CHECK: push 17
// CHECK-NEXT: jump [[TAIL:bb[0-9]+]]
// CHECK-NEXT: [[TAIL]] [cold]:
// CHECK-NEXT: push 32
// CHECK-NEXT: mstore
// CHECK-NEXT: push 0x4e487b71
// CHECK-NOT: push 0x4e487b71
// CHECK: push 18
// CHECK-NEXT: jump [[TAIL]]
// CHECK: push 50
// CHECK-NEXT: jump [[TAIL]]
contract SharedPanicTail {
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }

    function div(uint256 a, uint256 b) external pure returns (uint256) {
        return a / b;
    }

    function at(uint256 i) external pure returns (uint256) {
        uint256[3] memory xs = [uint256(1), 2, 3];
        return xs[i];
    }
}
