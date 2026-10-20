//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: level [0x0000000000000000000000000000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000000000000000000000000000003, 0x0000000000000000000000000000000000000000000000000000000000000004] => [0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0, 0x2e174c10e159ea99b867ce3205125c24a42d128804e4070ed6fcc8cc98166aa0]
//@ run-call-fail: scratchLevel() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A hash built in the scratch space writes below 0x40 only, so a loop that
// hashes there and checks indices against a reloaded length is versioned: the
// test also requires each reloaded word at or past 0x40, and the clone reads
// the lengths once, so their bounds checks go.
// CHECK-LABEL: fn @_level(
// CHECK: lt {{v[0-9]+}}, 64
// CHECK: jumpi {{v[0-9]+}}, [[FAST:bb[0-9]+]], [[SLOW:bb[0-9]+]]
// CHECK: [[FAST]]:
// CHECK: jump [[CLONE:bb[0-9]+]]
// CHECK: [[CLONE]]:
// CHECK-NOT: mload arg0
// CHECK: keccak256 0, 64

contract Test {
    function level(bytes32[] memory a) external pure returns (bytes32[] memory) {
        return _level(a);
    }

    // An array whose length word lies in the scratch space: the first hash
    // stores `a[0]`, one, over the length, so the next read is out of bounds.
    // The test sends the array to the original loop, which reads the length
    // again after each hash and fails there.
    function scratchLevel() external pure returns (bytes32[] memory) {
        bytes32[] memory a;
        assembly ("memory-safe") {
            mstore(0x00, 4)
            mstore(0x20, 1)
            a := 0x00
        }
        return _level(a);
    }

    function _level(bytes32[] memory a) private pure returns (bytes32[] memory out) {
        out = new bytes32[](a.length >> 1);
        for (uint256 i; i < out.length; ++i) {
            out[i] = keccak256(abi.encode(a[2 * i], a[2 * i + 1]));
        }
    }
}
