//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: decoded 0x0102 => 2
//@ run-call: lengthSet 0x0102, 1 => 1
//@ run-call-fail: lengthSet 0x0102, 0x10000000000000000
//@ run-call: offsetSet 0x0102, 0 => 2
//@ run-call: forwarded 0x0102 => 2

// ABI decoding has already checked an external argument's range, so copying
// it into memory does not check it again, and a length within calldata cannot
// overflow the padded allocation size.
// CHECK-LABEL: fn @decoded
// CHECK-NOT: {{0xffffffffffffffff$|shr 64,}}
// CHECK-NOT: 0x4e487b71
// CHECK: calldatacopy

// A slice whose length or offset assembly set keeps the copy's checks.
// CHECK-LABEL: fn @lengthSet
// CHECK: shr 64, arg1
// CHECK: sgt
// CHECK-LABEL: fn @offsetSet
// CHECK: shr 64,
// CHECK: sgt

// A public function also takes slices from internal calls, so it keeps them.
// CHECK-LABEL: fn @forwarded
// CHECK: shr 64,
// CHECK: sgt
contract CalldataBytesToMemoryChecks {
    function length(bytes memory b) internal pure returns (uint256) {
        return b.length;
    }

    function decoded(bytes calldata b) external pure returns (uint256) {
        return length(b);
    }

    function lengthSet(bytes calldata b, uint256 n) external pure returns (uint256) {
        assembly {
            b.length := n
        }
        return length(b);
    }

    function offsetSet(bytes calldata b, uint256 o) external pure returns (uint256) {
        assembly {
            b.offset := add(b.offset, o)
        }
        return length(b);
    }

    function forwarded(bytes calldata b) public pure returns (uint256) {
        return length(b);
    }
}
