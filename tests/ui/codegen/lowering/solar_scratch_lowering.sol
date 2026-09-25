//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// A `@custom:solar-scratch` block writes the free memory pointer it started
// with back when it ends, so each iteration's encoding reuses the memory of
// the one before, instead of growing memory with every iteration. The
// encoding is the block's last allocation, so its bump is dead before the
// restore, which then stores the pointer already there: the pointer never
// moves.
// CHECK-LABEL: fn @digests
// CHECK: mload 64
// CHECK-NOT: mstore 64
// CHECK: keccak256
// CHECK-NOT: mstore 64
// CHECK-LABEL: fn @plain
// CHECK: [[BASE:v[0-9]+]] = mload 64
// CHECK: mstore 64, {{v[0-9]+}}
// CHECK: keccak256
// CHECK-NOT: mstore 64, [[BASE]]
contract Test {
    function digests(uint256 n, bytes32 salt) public pure returns (bytes32 acc) {
        for (uint256 i; i < n; ++i) {
            /// @custom:solar-scratch
            {
                bytes memory encoded = abi.encode(i, salt, acc);
                acc = keccak256(encoded);
            }
        }
    }

    function plain(uint256 n, bytes32 salt) public pure returns (bytes32 acc) {
        for (uint256 i; i < n; ++i) {
            bytes memory encoded = abi.encode(i, salt, acc);
            acc = keccak256(encoded);
        }
    }
}
