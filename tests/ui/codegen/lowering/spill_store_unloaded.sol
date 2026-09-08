//@ revisions: ir run
//@[ir] compile-flags: -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: pick 3, 2 => 9
//@ run-call: pick 2, 3 => 9
//@ run-call: pick 0, 3 => 3
//@ run-call-fail: pick 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: pick 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// Both branch paths keep the product on the stack through the overflow check.
// Their sums share the final overflow check and return.
contract SpillStoreUnloaded {
    // CHECK: push 0xa62f4550
    // CHECK: eq
    // CHECK: mul
    // CHECK-NOT: mstore
    // CHECK: gt
    // CHECK-NEXT: iszero
    // CHECK-NEXT: push [[OTHER:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: add
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: pop
    // CHECK-NEXT: jump [[JOIN:bb[0-9]+]]
    // CHECK: [[JOIN]]:
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: lt
    // CHECK-NEXT: push [[OVERFLOW:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[OTHER]]:
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: add
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: pop
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: jump [[JOIN]]
    function pick(uint256 a, uint256 b) external pure returns (uint256) {
        uint256 c = a * b;
        if (a > b) {
            return c + a;
        }
        return c + b;
    }
}
