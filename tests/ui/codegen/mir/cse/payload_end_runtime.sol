//@ codegen-matrix: standard
//@ run-call: pad 0x0102030405, 1 => 0x02030405, 0
//@ run-call: pad 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 1 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 0
//@ run-call: pad 0x01, 1 => 0x, 0
//@ run-call-fail: pad 0x01, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: truncate 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 3 => 0x010203, 0
//@ run-call: truncate 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 40 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 0
//@ run-call: next => 1, 0x00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: wrap 0x0102, 115792089237316195423570985008687907853269984665640564039457584007913129639904 => 0
//@ run-call: wrap 0x0102, 1 => 1

// A zero word stored after a result's end, as a port pads it for legacy solc, leaves the result's
// length known to what follows, here its encoding; the bytes after its end read as zero even on
// dirty memory. The padding of one object can still reach the length word of the next, and with a
// length of `2^256 - 32`, its own.

contract Test {
    function pad(bytes memory s, uint256 start) external pure returns (bytes memory r, uint256 tail) {
        _dirty();
        uint256 n = s.length - start;
        r = new bytes(n);
        for (uint256 k; k < n; ++k) {
            r[k] = s[start + k];
        }
        _zeroPad(r);
        tail = _tail(r);
    }

    function truncate(bytes32 word, uint256 n) external pure returns (bytes memory r, uint256 tail) {
        _dirty();
        r = abi.encodePacked(word);
        if (n > r.length) n = r.length;
        assembly ("memory-safe") {
            mstore(r, n)
            mstore(add(add(r, 0x20), n), 0)
        }
        tail = _tail(r);
    }

    function next() external pure returns (uint256 first, uint256 second) {
        bytes memory r = new bytes(1);
        bytes memory x = new bytes(1);
        assembly ("memory-safe") {
            mstore(x, not(0))
        }
        _zeroPad(r);
        first = r.length;
        second = x.length;
    }

    function wrap(bytes memory s, uint256 n) external pure returns (uint256) {
        assembly ("memory-safe") {
            mstore(s, n)
            mstore(add(add(s, 0x20), n), 0)
        }
        return s.length;
    }

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
