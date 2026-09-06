//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

// The external wrapper of a public library function must decode a
// storage-reference parameter as its slot (one calldata word), not
// field-expand it like a memory struct. The body below reads and writes the
// struct's fields through the reference, so a wrapper that mis-decoded the
// parameter would address garbage. The one-word decode also keeps the
// wrapper's calldata-size check consistent with the linked-call encoding
// (`abi_head_size` = 32 for storage references).

library DataTypes {
    struct Reserve {
        uint128 a;
        uint128 b;
        uint256 total;
    }
}

library L {
    // CHECK-LABEL: @module L_runtime
    // CHECK: push 0xdef537e0
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[REJECT:bb[0-9]+]], [[GUARD:bb[0-9]+]]
    // CHECK-NEXT: [[GUARD]]:
    // CHECK-NEXT: push_immutable {{[0-9]+}}, 20
    // CHECK-NEXT: address
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[REJECT]], [[BODY:bb[0-9]+]]
    // CHECK-NEXT: [[BODY]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 68
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[REJECT]], [[DECODE:bb[0-9]+]]
    // CHECK-NEXT: [[DECODE]]:
    // CHECK-NEXT: push 1
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: add
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: sload
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: add
    // CHECK: gt
    // CHECK-NEXT: jumpi [[PANIC:bb[0-9]+]], [[STORE:bb[0-9]+]]
    // CHECK-NEXT: [[STORE]]:
    // CHECK: sstore
    // CHECK-NEXT: sload
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: sload
    // CHECK-NEXT: push 0
    // CHECK-NEXT: not
    // CHECK-NEXT: push 128
    // CHECK-NEXT: shr
    // CHECK: and
    // CHECK: add
    // CHECK: jumpi [[PANIC]], [[HIGH:bb[0-9]+]]
    // CHECK-NEXT: [[HIGH]]:
    // CHECK: push 128
    // CHECK-NEXT: shr
    // CHECK: and
    // CHECK: add
    // CHECK: jumpi [[PANIC]], [[RETURN:bb[0-9]+]]
    // CHECK-NEXT: [[RETURN]]:
    // CHECK: return
    // CHECK: [[PANIC]]{{( \[cold\])?}}:
    // CHECK-NEXT: push 0x4e487b71
    // CHECK: push 17
    // CHECK: revert
    function settle(DataTypes.Reserve storage r, uint256 amount) public returns (uint256) {
        r.total += amount;
        return r.total + uint256(r.a) + uint256(r.b);
    }
}
