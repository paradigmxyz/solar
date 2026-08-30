//@compile-flags: -Zdump=evm-ir-runtime --pretty-json
//@ filecheck:

// Both arms carry their sum into the join on the stack, and the join only
// returns it, so the definition-time spill store of the sum is never read
// back and is removed: the join runs its overflow check and then stores the
// return word directly.
contract SpillStoreUnloaded {
    // CHECK: push 0xa62f4550
    // CHECK: eq
    // CHECK: mul
    // CHECK: gt
    // CHECK-NEXT: push [[ARM:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: [[JOIN:bb[0-9]+]]:
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: lt
    // CHECK-NEXT: push [[OVERFLOW:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 128
    // CHECK-NEXT: return
    // CHECK: [[ARM]]:
    // CHECK: jump [[JOIN]]
    function pick(uint256 a, uint256 b) external pure returns (uint256) {
        uint256 c = a * b;
        if (a > b) {
            return c + a;
        }
        return c + b;
    }
}
