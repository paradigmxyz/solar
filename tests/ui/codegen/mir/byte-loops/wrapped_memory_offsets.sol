//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: bytesForward 0 => 0x112233
//@ run-call: bytesForward 1 => 0x111133
//@ run-call: bytesForward 2 => 0x111111
//@ run-call-fail: bytesForward 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: bytesBackward 2 => 0x333333
//@ run-call: wordsForward 2 => [11, 11, 11]
//@ run-call: bytesIntoHeader 0, true => true
//@ run-call: bytesIntoHeader 1, true => true
//@ run-call-fail: bytesIntoHeader 2, true => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: bytesIntoHeader 2, false => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: calldataIntoHeader 0x0102, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => true
//@ run-call: calldataIntoHeader 0x0102, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => true
//@ run-call-fail: calldataIntoHeader 0x0102, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: freshWrapped 0 => 0x
//@ run-call: freshWrapped 2 => 0x0000
//@ run-call: freshWrapped 33 => 0x000000000000000000000000000000000000000000000000000000000000000000

// An assembly-forged length admits a large offset that wraps the effective data address
// below the source's header. Overlapping copies must retain the loop's read-after-write
// behavior even though the header itself lies beyond the destination range.
contract Test {
    function bytesForward(uint256 count) external pure returns (bytes memory destination) {
        destination = hex"112233";
        bytes memory source = new bytes(0);
        uint256 offset;
        assembly {
            mstore(source, not(0))
            offset := sub(destination, source)
        }
        _copyBytes(source, destination, offset, 1, count);
    }

    function bytesBackward(uint256 count) external pure returns (bytes memory destination) {
        destination = hex"112233";
        bytes memory source = new bytes(0);
        uint256 offset;
        assembly {
            mstore(source, not(0))
            offset := sub(add(destination, 1), source)
        }
        _copyBytesDown(source, destination, offset, 0, count);
    }

    function wordsForward(uint256 count) external pure returns (uint256[] memory destination) {
        destination = new uint256[](3);
        destination[0] = 11;
        destination[1] = 22;
        destination[2] = 33;
        uint256[] memory source = new uint256[](0);
        uint256 offset;
        assembly {
            mstore(source, not(0))
            offset := shr(5, sub(destination, source))
        }
        _copyWords(source, destination, offset, 1, count);
    }

    // The first store changes the destination's length, so the next destination check must
    // panic. This also applies when the source and destination are the same object.
    function bytesIntoHeader(uint256 count, bool sameObject) external pure returns (bool) {
        bytes memory source = hex"0102";
        bytes memory destination = new bytes(2);
        if (sameObject) destination = source;
        assembly {
            mstore(destination, not(0))
        }
        if (sameObject) {
            _copyBytesSelf(destination, count);
        } else {
            _copyBytes(source, destination, 0, type(uint256).max - 31, count);
        }
        return true;
    }

    // The source header is above the next allocation, but its wrapped data address points
    // into that allocation. Removing the allocation's zeroing would expose the dirty bytes.
    function freshWrapped(uint256 count) external pure returns (bytes memory) {
        bytes memory source;
        uint256 offset;
        assembly {
            let next := mload(0x40)
            source := add(next, 0x100)
            mstore(source, not(0))
            mstore(add(next, 0x20), not(0))
            offset := sub(next, source)
        }
        return _freshBytes(source, offset, count);
    }

    // CHECK-LABEL: fn @_copyBytes(
    // CHECK: [[SRC_BASE:v[0-9]+]] = add arg0, arg2
    // CHECK-NEXT: [[FROM:v[0-9]+]] = add [[SRC_BASE]], 32
    // CHECK-NEXT: [[DST_BASE:v[0-9]+]] = add arg1, arg3
    // CHECK-NEXT: [[TO:v[0-9]+]] = add [[DST_BASE]], 32
    // CHECK: [[SRC_DATA:v[0-9]+]] = add arg0, 32
    // CHECK-NEXT: {{v[0-9]+}} = lt [[FROM]], [[SRC_DATA]]
    // CHECK: [[DST_DATA:v[0-9]+]] = add arg1, 32
    // CHECK-NEXT: {{v[0-9]+}} = lt [[TO]], [[DST_DATA]]
    // CHECK: jumpi
    // CHECK: mcopy [[TO]], [[FROM]], arg4
    // CHECK: mstore8
    /// @custom:solar-noinline
    function _copyBytes(bytes memory source, bytes memory destination, uint256 sOff, uint256 dOff, uint256 count)
        private pure
    {
        for (uint256 k; k < count; ++k) {
            destination[dOff + k] = source[sOff + k];
        }
    }

    /// @custom:solar-noinline
    function _copyBytesDown(bytes memory source, bytes memory destination, uint256 sOff, uint256 dOff, uint256 count)
        private pure
    {
        for (uint256 k = count; k > 0; --k) {
            destination[dOff + k - 1] = source[sOff + k - 1];
        }
    }

    /// @custom:solar-noinline
    function _copyWords(uint256[] memory source, uint256[] memory destination, uint256 sOff, uint256 dOff, uint256 count)
        private pure
    {
        for (uint256 k; k < count; ++k) {
            destination[dOff + k] = source[sOff + k];
        }
    }

    // The wrap guard must select zeroing before the slow copy, not only choose between
    // the loop and MCOPY after the allocation's zeroing has already been skipped.
    // CHECK-LABEL: fn @_freshBytes(
    // CHECK: [[BASE:v[0-9]+]] = add arg0, arg1
    // CHECK-NEXT: [[FROM:v[0-9]+]] = add [[BASE]], 32
    // CHECK: [[DATA:v[0-9]+]] = add arg0, 32
    // CHECK-NEXT: [[WRAPS:v[0-9]+]] = lt [[FROM]], [[DATA]]
    // CHECK-NEXT: [[KEEP:v[0-9]+]] = or {{v[0-9]+}}, [[WRAPS]]
    // CHECK-NEXT: jumpi [[KEEP]], [[SLOW:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: mcopy
    // CHECK: [[SLOW]]:
    // CHECK: calldatacopy
    // CHECK: mstore8
    /// @custom:solar-noinline
    function _freshBytes(bytes memory source, uint256 offset, uint256 count)
        private pure returns (bytes memory destination)
    {
        destination = new bytes(count);
        for (uint256 k; k < count; ++k) {
            destination[k] = source[offset + k];
        }
        assembly ("memory-safe") {
            mstore(add(add(destination, 32), count), 0)
        }
    }

    /// @custom:solar-noinline
    function _copyBytesSelf(bytes memory buffer, uint256 count) private pure {
        for (uint256 k; k < count; ++k) {
            buffer[type(uint256).max - 31 + k] = buffer[k];
        }
    }

    // Decoded calldata cannot wrap, but the memory destination can still overwrite its own
    // length header. Its input payload supplies the forged length, so no preceding store can
    // be forwarded into the loop. The second iteration must fail after the first shrinks it.
    function calldataIntoHeader(bytes calldata source, bytes memory destination, uint256 count)
        external pure returns (bool)
    {
        assembly {
            destination := add(destination, 32)
        }
        for (uint256 k; k < count; ++k) {
            destination[type(uint256).max - 31 + k] = source[k];
        }
        return true;
    }
}
