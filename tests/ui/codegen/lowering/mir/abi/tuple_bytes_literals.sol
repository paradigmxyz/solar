//@ compile-flags: -Zdump=mir
//@ filecheck:
//@ run-call: prefix 0xaaaaaa => 0x3078aa
//@ run-call-fail: prefix 0xaa => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: words => 0x30, 0x7879

// A string literal assigned to fixed bytes in a tuple assignment is the word the
// target holds, as in a single assignment, rather than a string object that a
// conversion reads back, so nothing is allocated for it.
contract TupleBytesLiterals {
    // CHECK-LABEL: fn @prefix(
    // CHECK: calldatacopy
    // CHECK-NOT: mstore 64
    // CHECK: mstore8 {{v[0-9]+}}, 120
    // CHECK-NEXT: mstore8 {{v[0-9]+}}, 48
    function prefix(bytes memory out) external pure returns (bytes memory) {
        (out[0], out[1]) = ("0", "x");
        return out;
    }

    // CHECK-LABEL: fn @words(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: mstore 0, 0x3000000000000000000000000000000000000000000000000000000000000000
    // CHECK-NEXT: mstore 32, 0x7879000000000000000000000000000000000000000000000000000000000000
    // CHECK-NEXT: returndata 0, 64
    function words() external pure returns (bytes1 a, bytes2 b) {
        (a, b) = ("0", "xy");
    }
}
