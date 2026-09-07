//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

// `abi.encodePacked(...)` may include a `bytes`/`string` calldata argument,
// which is packed as its raw data (no length prefix, no padding). The calldata
// is copied into a `[len][data]` memory buffer and then packed like any other
// dynamic bytes value. Used by nitro-contracts MockRollupEventInbox. The packed
// bytes (hashed here) are verified equal to solc 0.8.30 separately.

// CHECK-LABEL: @module P_runtime
// CHECK: callvalue
// CHECK-NEXT: jumpi [[BAD:bb[0-9]+]], [[DISPATCH:bb[0-9]+]]
// CHECK: [[DISPATCH]]:
// CHECK: push 0x21bd63cb
// CHECK-NEXT: eq
// CHECK-NEXT: jumpi [[H_ENTRY:bb[0-9]+]], [[H2_SELECTOR:bb[0-9]+]]
// CHECK: [[H2_SELECTOR]]:
// CHECK-NEXT: push 0xf1245422
// CHECK-NEXT: sub
// CHECK-NEXT: jumpi [[BAD]], [[H2_ENTRY:bb[0-9]+]]
// CHECK: [[H2_ENTRY]]:
// CHECK-NEXT: calldatasize
// CHECK-NEXT: push 68
// CHECK-NEXT: gt
// CHECK-NEXT: jumpi [[BAD]], [[ADDRESS:bb[0-9]+]]
// CHECK: [[ADDRESS]]:
// CHECK-NEXT: push 36
// CHECK-NEXT: calldataload
// CHECK-NEXT: push 160
// CHECK-NEXT: shr
// CHECK-NEXT: jumpi [[BAD]], [[H2_DECODE:bb[0-9]+]]
// CHECK: [[H2_DECODE]]:
// CHECK-NEXT: push [[H2_BODY:bb[0-9]+]]
// CHECK-NEXT: push 4
// CHECK-NEXT: jump [[DECODE:bb[0-9]+]]
contract P {
    // The Gas policy keeps each final ADD/hash/return inline. Size mode continues
    // sharing that tail; changing this Gas contract is an explicit policy choice.
    // h appends a whole uint256 word after exactly the unpadded calldata bytes.
    // CHECK: [[H_BODY:bb[0-9]+]]:
    // CHECK-NEXT: push 32
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: add
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mload
    // CHECK: calldatacopy
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: add
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: add
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: sub
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: keccak256
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    function h(bytes calldata a, uint256 x) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, x));
    }

    // h2 prefixes three bytes, then appends the left-aligned twenty-byte address.
    // CHECK: [[H2_BODY]]:
    // CHECK-NEXT: push 32
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: add
    // CHECK-NEXT: push 0
    // CHECK-NEXT: not
    // CHECK-NEXT: push 96
    // CHECK-NEXT: shr
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: and
    // CHECK-NEXT: push 64
    // CHECK-NEXT: mload
    // CHECK-NEXT: push 0x707265
    // CHECK-NEXT: push 232
    // CHECK-NEXT: shl
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 3
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: add
    // CHECK: calldatacopy
    // CHECK: push 96
    // CHECK-NEXT: shl
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 20
    // CHECK-NEXT: add
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: sub
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: keccak256
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[BAD]] [cold]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert

    // Both validated static heads feed the same bounded dynamic-slice decoder.
    // CHECK: [[H_ENTRY]]:
    // CHECK-NEXT: pop
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: push 68
    // CHECK-NEXT: gt
    // CHECK-NEXT: jumpi [[BAD]], [[H_DECODE:bb[0-9]+]]
    // CHECK: [[H_DECODE]]:
    // CHECK-NEXT: push [[H_BODY]]
    // CHECK-NEXT: push 4
    // CHECK-NEXT: jump [[DECODE]]
    // CHECK: [[DECODE]]:
    // CHECK-NEXT: calldatasize
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: push 4
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: add
    // CHECK-NEXT: push 0
    // CHECK-NEXT: not
    // CHECK-NEXT: push 192
    // CHECK-NEXT: shr
    // CHECK-NEXT: push 36
    // CHECK-NEXT: dup 4
    // CHECK-NEXT: add
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: swap 3
    // CHECK-NEXT: gt
    // CHECK-NEXT: dup 4
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: gt
    // CHECK-NEXT: or
    // CHECK-NEXT: dup 3
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: swap 2
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: swap 3
    // CHECK-NEXT: swap 4
    // CHECK-NEXT: sub
    // CHECK-NEXT: lt
    // CHECK-NEXT: or
    // CHECK-NEXT: jumpi [[BAD]], [[DECODE_DONE:bb[0-9]+]]
    // CHECK: [[DECODE_DONE]]:
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: jump
    function h2(bytes calldata a, address b) external pure returns (bytes32) {
        return keccak256(abi.encodePacked("pre", a, b));
    }
}
