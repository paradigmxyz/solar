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

// An allocation size that nothing shares keeps the single rounding of `(len + 63) & ~31`.
// CHECK-LABEL: fn @alloc(
// CHECK: = add arg0, 63
// CHECK-NOT: = add arg0, 31

contract Test {
    event E(string s);

    function relay(string memory s) external returns (string memory) {
        emit E(s);
        return s;
    }

    function alloc(uint256 n) external pure returns (uint256) {
        bytes memory b = new bytes(n);
        return b.length;
    }
}
