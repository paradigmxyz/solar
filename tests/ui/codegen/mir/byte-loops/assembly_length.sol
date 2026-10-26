//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: copy 0x0102030405, 1 => 0x02030405, 0
//@ run-call: copy 0x, 0 => 0x, 0
//@ run-call: copy 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 0 => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 0
//@ run-call-fail: copy 0x0102, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: word 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 40 => 0x00000000000000000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 0
//@ run-call-fail: word 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 31 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: fill 32 => 0x6161616161616161616161616161616161616161616161616161616161616161, 0
//@ run-call-fail: fill 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: grownPast() => 0x0101010101010101010101010101010101010101010101010101010101010101

// A buffer allocated in whole words takes its length from a trusted `mstore(b, n)` after
// `new bytes`, as a port that must leave its result zero right padded under legacy solc
// allocates it. The checks of the loops that write it compare with that stored length, the last
// one the allocation's block leaves, so they still become word operations.

// A copy into the buffer is one `mcopy`, behind the overlap test an argument source takes, before
// the result's own encoding copies it again.
// CHECK-LABEL: fn @copy(
// CHECK: mcopy
// CHECK: mstore8
// CHECK: mcopy

// A word written a byte at a time is one word store.
// CHECK-LABEL: fn @word(
// CHECK-NOT: mstore8
// CHECK: ret

// A fill checks against the stored length, not the whole words allocated.
// CHECK-LABEL: fn @fill(
// CHECK-NOT: mstore8
// CHECK: ret

contract Test {
    function copy(bytes memory src, uint256 start) external pure returns (bytes memory out, uint256 tail) {
        _dirty();
        uint256 n = src.length - start;
        out = new bytes((n + 31) & ~uint256(31));
        assembly ("memory-safe") {
            mstore(out, n)
        }
        for (uint256 k; k < n; ++k) {
            out[k] = src[start + k];
        }
        tail = _tail(out);
    }

    function word(bytes32 value, uint256 n) external pure returns (bytes memory out, uint256 tail) {
        _dirty();
        out = new bytes((n + 31) & ~uint256(31));
        assembly ("memory-safe") {
            mstore(out, n)
        }
        for (uint256 k; k < 32; ++k) {
            out[k + 8] = value[k];
        }
        tail = _tail(out);
    }

    function fill(uint256 n) external pure returns (bytes memory out, uint256 tail) {
        _dirty();
        out = new bytes((n + 31) & ~uint256(31));
        assembly ("memory-safe") {
            mstore(out, n)
        }
        for (uint256 k; k < 32; ++k) {
            out[k] = 0x61;
        }
        tail = _tail(out);
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

    /// A source length grown past its allocation runs into the next one, where the destination
    /// starts inside the source range: the loop copies bytes it already wrote, so it stays.
    function grownPast() external pure returns (bytes32 word) {
        bytes memory a = new bytes(1);
        bytes memory b = new bytes(64);
        assembly {
            mstore(a, 200)
            mstore(add(a, 0x20), not(0))
            mstore(add(a, 0x40), 0x0101010101010101010101010101010101010101010101010101010101010101)
        }
        for (uint256 k; k < 64; ++k) {
            b[k] = a[k + 40];
        }
        assembly {
            word := mload(add(b, 0x40))
        }
    }
}
