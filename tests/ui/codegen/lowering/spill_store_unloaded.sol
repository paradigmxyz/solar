//@compile-flags: -Zdump=evm-ir-runtime --pretty-json
//@ filecheck:

// Both branch paths select their operand, then share the add, overflow check,
// and return.
contract SpillStoreUnloaded {
    // CHECK: push 0xa62f4550
    // CHECK: eq
    // CHECK: mul
    // CHECK: gt
    // CHECK-NEXT: push [[JOIN:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: jump [[JOIN:bb[0-9]+]]
    // CHECK: [[JOIN]]:
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK-NEXT: add
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: pop
    // CHECK-NEXT: push 160
    // CHECK-NEXT: mload
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: lt
    // CHECK-NEXT: push [[OVERFLOW:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK-NEXT: push 128
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 128
    // CHECK-NEXT: return
    function pick(uint256 a, uint256 b) external pure returns (uint256) {
        uint256 c = a * b;
        if (a > b) {
            return c + a;
        }
        return c + b;
    }
}
