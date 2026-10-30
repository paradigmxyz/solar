//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: relay 0x0102030405 => 0x0102030405

// The event and the return encode the string that was decoded and copied into a fresh
// allocation. Both read its length from the word the decoder stored, so the allocation size, the
// event's padding, and the return's padding come from one padded length, and nothing loads the
// length word again.
// CHECK-LABEL: fn @relay(
// CHECK: [[LEN:v[0-9]+]] = calldataload {{v[0-9]+}}
// CHECK: = add [[LEN]], 31
// CHECK-NOT: = mload {{v[0-9]+}}
// CHECK-NOT: = add [[LEN]], 31
// CHECK-NOT: = add [[LEN]], 63
// CHECK: returndata

contract Test {
    event E(string s);

    function relay(string memory s) external returns (string memory) {
        emit E(s);
        return s;
    }
}
