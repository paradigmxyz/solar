//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: load32 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 2 => 0x02030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021
//@ run-call-fail: load32 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: load32 0x0001, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: load32 0x0001, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: load4 0x0a0b0c0d0e, 1 => 0x0b0c0d0e
//@ run-call-fail: load4 0x0a0b0c0d0e, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: store32 0x00000000000000000000000000000000000000000000000000000000000000000000, 1, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2000
//@ run-call-fail: store32 0x00, 0, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: store2 0xaaaaaaaa, 1, 0x0102 => 0xaa0102aa
//@ run-call-fail: store2 0xaaaaaaaa, 3, 0x0102 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: slice 0x0a0b0c0d0e, 1, 3 => 0x0b0c0d
//@ run-call: slice 0x0a0b, 7, 0 => 0x
//@ run-call-fail: slice 0x0a0b0c0d0e, 3, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A gather of a whole word is one range check and one load.
// CHECK-LABEL: fn @load32(
// CHECK-NOT: byte 0
// CHECK: mload
// CHECK: ret

// Below a word, the load keeps the leading bytes.
// CHECK-LABEL: fn @load4(
// CHECK-NOT: byte 0
// CHECK: and {{v[0-9]+}}, 0xffffffff00000000000000000000000000000000000000000000000000000000

// A scatter of a whole word is one store.
// CHECK-LABEL: fn @store32(
// CHECK-NOT: mstore8
// CHECK: mstore

// Below a word, the store keeps the bytes after the written ones.
// CHECK-LABEL: fn @store2(
// CHECK-NOT: mstore8
// CHECK: mstore

// A copy into a fresh buffer is an `mcopy`, behind a test that keeps the loop for a source
// that the buffer overlaps, as a memory argument may.
// CHECK-LABEL: fn @slice(
// CHECK: mcopy

contract Test {
    function load32(bytes memory b, uint256 offset) external pure returns (bytes32 result) {
        for (uint256 k; k < 32; ++k) {
            result |= bytes32(b[offset + k]) >> (8 * k);
        }
    }

    function load4(bytes memory b, uint256 offset) external pure returns (bytes4 result) {
        for (uint256 k; k < 4; ++k) {
            result |= bytes4(b[offset + k]) >> (8 * k);
        }
    }

    function store32(bytes memory b, uint256 offset, bytes32 value) external pure returns (bytes memory) {
        for (uint256 k; k < 32; ++k) {
            b[offset + k] = value[k];
        }
        return b;
    }

    function store2(bytes memory b, uint256 offset, bytes2 value) external pure returns (bytes memory) {
        for (uint256 k; k < 2; ++k) {
            b[offset + k] = value[k];
        }
        return b;
    }

    function slice(bytes memory b, uint256 offset, uint256 count) external pure returns (bytes memory result) {
        result = new bytes(count);
        for (uint256 k; k < count; ++k) {
            result[k] = b[offset + k];
        }
    }
}
