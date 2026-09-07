//@compile-flags: -Zdump=evm-ir-runtime --pretty-json
//@ filecheck:

// Both branch paths select their operand, then share the add, overflow check,
// and return.
contract SpillStoreUnloaded {
    // The product stays on the physical stack. A Boolean chooses calldata
    // offset 4 or 36, then one ADD, overflow branch and Return serve both inputs.
    // CHECK-LABEL: @module SpillStoreUnloaded_runtime
    // CHECK: push 0xa62f4550
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[INVALID:bb[0-9]+]], [[DECODE:bb[0-9]+]]
    // CHECK: [[DECODE]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 68
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[INVALID]], [[PRODUCT:bb[0-9]+]]
    // CHECK: [[PRODUCT]]:
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: mul
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: iszero
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: div
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: eq
    // CHECK-NEXT: or
    // CHECK-NEXT: jumpi [[JOIN:bb[0-9]+]], [[OVERFLOW:bb[0-9]+]]
    // CHECK: [[OVERFLOW]] [cold]:
    // CHECK-NEXT: push 0x4e487b71
    // CHECK-NEXT: push 224
    // CHECK-NEXT: shl
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 17
    // CHECK-NEXT: push 4
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 36
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    // CHECK: [[JOIN]]:
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: gt
    // CHECK-NEXT: push 5
    // CHECK-NEXT: shl
    // CHECK-NEXT: push 36
    // CHECK-NEXT: sub
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: add
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[OVERFLOW]], [[RETURN:bb[0-9]+]]
    // CHECK-NEXT: [[RETURN]]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[INVALID]] [cold]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    // CHECK-NOT: {{^  }}add
    // CHECK-NOT: {{^  }}return
    function pick(uint256 a, uint256 b) external pure returns (uint256) {
        uint256 c = a * b;
        if (a > b) {
            return c + a;
        }
        return c + b;
    }
}
