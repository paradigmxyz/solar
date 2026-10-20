//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: whole 0x0102030405 => 0x0102030405
//@ run-call: whole 0x => 0x
//@ run-call: whole 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627 => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627
//@ run-call: rest 0x0102030405, 1 => 0x02030405
//@ run-call: rest 0x01, 1 => 0x
//@ run-call-fail: rest 0x01, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: rest 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 3 => 0x030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627
//@ run-call: slice 0x0102030405, 1, 3 => 0x0203, 0
//@ run-call: slice 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 2, 39 => 0x02030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20212223242526, 0
//@ run-call: slice 0x0102, 5, 9 => 0x, 0
//@ run-call: overlapWhole 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 0 => 0x00000000000000000000000000000000000000000000000000000000000000000000000000000000
//@ run-call: overlapWhole 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 32 => 0x00000000000000000000000000000000000000000000000000000000000000280000000000000000
//@ run-call: overlapWhole 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 64 => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f0000000000000000
//@ run-call: overlapWhole 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 96 => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627
//@ run-call-fail: overlapRest 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 3, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: overlapRest 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 3, 32 => 0x00000000000000000000000000000000000000000000000000000000250000000000000000
//@ run-call: overlapRest 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 3, 48 => 0x030405060708090a0b0c0d0e0f000000000000000000000000000000000000000000000000
//@ run-call: overlapRest 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 3, 96 => 0x030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627

// A copy into a buffer `new bytes(n)` just allocated overwrites its zeroing. Unless a zero word
// stored after the copied bytes clears the padding, a zero store to the allocation's last word
// does, before the copy. An argument source may overlap the buffer, which the loop would read
// zeroed: the copy runs unzeroed only when the source stays off the allocation, and when the
// source's length from before the allocation proves the copy's range, off the free memory
// pointer below the zero slot too, and the range check goes. Otherwise the buffer is zeroed late
// and the copy runs as before. A free memory pointer moved onto the source sends it that way.

// CHECK-LABEL: fn @whole(
// CHECK: lt {{v[0-9]+}}, 96
// CHECK: mstore {{v[0-9]+}}, 0
// CHECK-NEXT: mcopy
// CHECK: calldatacopy
// CHECK: mcopy
// CHECK: mstore8

// CHECK-LABEL: fn @rest(
// CHECK: lt {{v[0-9]+}}, 96
// CHECK-NOT: mstore {{v[0-9]+}}, 0
// CHECK: mcopy
// CHECK: calldatacopy

// CHECK-LABEL: fn @slice(
// CHECK-NOT: lt {{v[0-9]+}}, 96
// CHECK: mload
// CHECK-NOT: mstore {{v[0-9]+}}, 0
// CHECK: mcopy
// CHECK: calldatacopy

contract Test {
    function whole(bytes memory s) external pure returns (bytes memory r) {
        uint256 n = s.length;
        r = new bytes(n);
        for (uint256 k; k < n; ++k) {
            r[k] = s[k];
        }
    }

    function rest(bytes memory s, uint256 start) external pure returns (bytes memory r) {
        uint256 n = s.length - start;
        r = new bytes(n);
        for (uint256 k; k < n; ++k) {
            r[k] = s[start + k];
        }
        _zeroPad(r);
    }

    function slice(bytes memory s, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory r, uint256 tail)
    {
        _dirty();
        if (end > s.length) end = s.length;
        if (start > end) start = end;
        r = new bytes(end - start);
        _copy(r, s, start, end - start);
        _zeroPad(r);
        tail = _tail(r);
    }

    function overlapWhole(bytes memory s, uint256 shift) external pure returns (bytes memory) {
        assembly ("memory-safe") {
            mstore(0x40, add(s, shift))
        }
        return _whole(s);
    }

    function overlapRest(bytes memory s, uint256 start, uint256 shift)
        external
        pure
        returns (bytes memory)
    {
        assembly ("memory-safe") {
            mstore(0x40, add(s, shift))
        }
        return _rest(s, start);
    }

    /// @custom:solar-noinline
    function _whole(bytes memory s) private pure returns (bytes memory r) {
        uint256 n = s.length;
        r = new bytes(n);
        for (uint256 k; k < n; ++k) {
            r[k] = s[k];
        }
    }

    /// @custom:solar-noinline
    function _rest(bytes memory s, uint256 start) private pure returns (bytes memory r) {
        uint256 n = s.length - start;
        r = new bytes(n);
        for (uint256 k; k < n; ++k) {
            r[k] = s[start + k];
        }
        _zeroPad(r);
    }

    /// @custom:solar-inline
    function _copy(bytes memory dst, bytes memory src, uint256 from, uint256 count) private pure {
        for (uint256 k; k < count; ++k) {
            dst[k] = src[from + k];
        }
    }

    /// @custom:solar-inline
    function _zeroPad(bytes memory b) private pure {
        assembly ("memory-safe") {
            mstore(add(add(b, 0x20), mload(b)), 0)
        }
    }

    function _dirty() private pure {
        assembly ("memory-safe") {
            let m := mload(0x40)
            for { let i := 0 } lt(i, 0x400) { i := add(i, 0x20) } { mstore(add(m, i), not(0)) }
        }
    }

    /// The bytes after `b`'s end in its last word, which must read as zero.
    function _tail(bytes memory b) private pure returns (uint256 tail) {
        assembly ("memory-safe") {
            let length := mload(b)
            if and(length, 0x1f) {
                let last := mload(add(add(b, 0x20), and(length, not(0x1f))))
                tail := shl(mul(8, and(length, 0x1f)), last)
            }
        }
    }
}
