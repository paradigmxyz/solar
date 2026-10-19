//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: fromCalldata 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 2 => 0x02030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021
//@ run-call-fail: fromCalldata 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: storeSome 0xaaaaaaaa, 0x0102000000000000000000000000000000000000000000000000000000000000, 2 => 0x0102aaaa
//@ run-call: storeSome 0xaaaaaaaa, 0x0102000000000000000000000000000000000000000000000000000000000000, 0 => 0xaaaaaaaa
//@ run-call-fail: storeSome 0xaaaaaaaa, 0x0102000000000000000000000000000000000000000000000000000000000000, 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: storeSome 0x00000000000000000000000000000000000000000000000000000000000000000000, 0x0102000000000000000000000000000000000000000000000000000000000000, 33 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: loadSome 0x0a0b0c0d0e, 1, 3 => 0x0b0c0d0000000000000000000000000000000000000000000000000000000000
//@ run-call: loadSome 0x0a0b, 7, 0 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: loadSome 0x0a0b0c0d0e, 3, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: within 0x0102030405, 0, 1, 3 => 0x0101010105
//@ run-call: within 0x0102030405, 1, 0, 3 => 0x0203040405
//@ run-call: within 0x0102030405, 0, 0, 5 => 0x0102030405
//@ run-call-fail: within 0x0102030405, 3, 0, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: between 0x0102030405, 0xaaaaaaaaaa, 1, 3 => 0xaa020304aa
//@ run-call-fail: between 0x0102030405, 0xaaaaaa, 1, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: down 0x0102030405, 0, 1, 3 => 0x0101020305
//@ run-call: down 0x0102030405, 1, 0, 3 => 0x0404040405
//@ run-call-fail: down 0x0102030405, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: down 0x0102030405, 3, 0, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: down 0x0102030405, 9, 9, 0 => 0x0102030405
//@ run-call: sliceCalldata 0x0a0b0c0d0e, 1, 3 => 0x0b0c0d
//@ run-call-fail: sliceCalldata 0x0a0b0c0d0e, 3, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: fresh 0x0102030405, 1, 3 => 0x00020304
//@ run-call-fail: fresh 0x0102030405, 3, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: viaHelper 0x0102030405, 2, 3 => 0x030405
//@ run-call: zeroSome 0x0102030405060708090a, 2 => 0x0102000000000000000a
//@ run-call-fail: zeroSome 0x0102030405060708, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: fillSome 0x0102030405, 0x61 => 0x6161616105
//@ run-call-fail: fillSome 0x010203, 0x61 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: viaHelper 0x0102030405, 2, 4 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// A gather from calldata is one `calldataload`.
// CHECK-LABEL: fn @fromCalldata(
// CHECK-NOT: byte 0
// CHECK: calldataload

// A scatter of a dynamic count merges the leading bytes into one word.
// CHECK-LABEL: fn @storeSome(
// CHECK-NOT: mstore8
// CHECK: mstore

// A gather of a dynamic count keeps at most a word.
// CHECK-LABEL: fn @loadSome(
// CHECK-NOT: byte 0
// CHECK: select

// A copy within one object keeps the loop for a destination that leads into the source.
// CHECK-LABEL: fn @within(
// CHECK: mcopy
// CHECK: mstore8

// A copy between two arguments takes the same guard.
// CHECK-LABEL: fn @between(
// CHECK: mcopy

// A downward copy keeps the loop for a source that starts inside the destination.
// CHECK-LABEL: fn @down(
// CHECK: mcopy
// CHECK: mstore8

// A copy out of calldata is one `calldatacopy`.
// CHECK-LABEL: fn @sliceCalldata(
// CHECK-NOT: mstore8
// CHECK: calldatacopy

// A fresh destination's checks compare with the length its allocation stored, and the copy
// needs no overlap guard.
// CHECK-LABEL: fn @fresh(
// CHECK-NOT: mstore8
// CHECK: mcopy
// CHECK-NOT: mstore8

// A `@custom:solar-inline` helper meets the caller's fresh destination before this pass runs.
// CHECK-LABEL: fn @viaHelper(
// CHECK-NOT: mstore8
// CHECK: mcopy
// CHECK-NOT: mstore8

// A constant byte fills the range with one masked word store.
// CHECK-LABEL: fn @zeroSome(
// CHECK-NOT: mstore8
// CHECK: mstore

// So does a byte the loop does not vary.
// CHECK-LABEL: fn @fillSome(
// CHECK-NOT: mstore8
// CHECK: mul
// CHECK: mstore

contract Test {
    function fromCalldata(bytes calldata b, uint256 offset) external pure returns (bytes32 result) {
        for (uint256 k; k < 32; ++k) {
            result |= bytes32(b[offset + k]) >> (8 * k);
        }
    }

    function storeSome(bytes memory b, bytes32 value, uint256 count) external pure returns (bytes memory) {
        for (uint256 k; k < count; ++k) {
            b[k] = value[k];
        }
        return b;
    }

    function loadSome(bytes memory b, uint256 offset, uint256 count) external pure returns (bytes32 result) {
        for (uint256 k; k < count; ++k) {
            result |= bytes32(b[offset + k]) >> (8 * k);
        }
    }

    function within(bytes memory b, uint256 from, uint256 to, uint256 count) external pure returns (bytes memory) {
        for (uint256 k; k < count; ++k) {
            b[to + k] = b[from + k];
        }
        return b;
    }

    function between(bytes memory src, bytes memory dst, uint256 offset, uint256 count) external pure returns (bytes memory) {
        for (uint256 k; k < count; ++k) {
            dst[offset + k] = src[offset + k];
        }
        return dst;
    }

    function down(bytes memory b, uint256 from, uint256 to, uint256 count) external pure returns (bytes memory) {
        for (uint256 k = count; k > 0; --k) {
            b[to + k - 1] = b[from + k - 1];
        }
        return b;
    }

    function sliceCalldata(bytes calldata b, uint256 offset, uint256 count) external pure returns (bytes memory result) {
        result = new bytes(count);
        for (uint256 k; k < count; ++k) {
            result[k] = b[offset + k];
        }
    }

    function fresh(bytes memory src, uint256 from, uint256 count) external pure returns (bytes memory out) {
        out = new bytes(count + 1);
        for (uint256 k; k < count; ++k) {
            out[1 + k] = src[from + k];
        }
    }

    function viaHelper(bytes memory src, uint256 from, uint256 count) external pure returns (bytes memory out) {
        out = new bytes(count);
        _copy(out, 0, src, from, count);
    }

    function zeroSome(bytes memory b, uint256 offset) external pure returns (bytes memory) {
        for (uint256 k; k < 7; ++k) {
            b[offset + k] = 0;
        }
        return b;
    }

    function fillSome(bytes memory b, bytes1 c) external pure returns (bytes memory) {
        for (uint256 k; k < 4; ++k) {
            b[k] = c;
        }
        return b;
    }

    /// @custom:solar-inline
    function _copy(bytes memory dst, uint256 at, bytes memory src, uint256 from, uint256 count) private pure {
        for (uint256 k; k < count; ++k) {
            dst[at + k] = src[from + k];
        }
    }
}
