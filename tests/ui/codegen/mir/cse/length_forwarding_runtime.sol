//@ codegen-matrix: standard
//@ run-call: relay 0x0102030405 => 0x0102030405
//@ run-call: relay 0x => 0x
//@ run-call: relay 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
//@ run-call: twice 0x0102030405 => 0x0102030405
//@ run-call: twice 0x => 0x
//@ run-call: shorten 0x0102030405060708090a, 5 => 0x0102030405
//@ run-call: shorten 0x0102030405060708090a, 0 => 0x
//@ run-call: shorten 0x, 0 => 0x
//@ run-call: shortenThroughFmp 0x0102030405060708090a => 0x0102030405
//@ run-call: shortenThroughFmp 0x01 => 0x0100000000
//@ run-call: rewind 0x0102030405 => 0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000

// The length of a decoded or built object is loaded again after an event encodes it and again
// when the return encodes it. Those loads forward to the length that was stored, unless a write
// in between reaches the length word.

contract Test {
    event E(bytes s);

    function relay(bytes memory s) external returns (bytes memory) {
        emit E(s);
        return s;
    }

    function twice(bytes memory s) external returns (bytes memory) {
        emit E(s);
        emit E(s);
        return s;
    }

    // Assembly overwrites the length word, so the return encodes the new length.
    function shorten(bytes memory s, uint256 n) external returns (bytes memory) {
        emit E(s);
        assembly ("memory-safe") {
            mstore(s, n)
        }
        return s;
    }

    // The store lands on the length word through the free memory pointer, below it by the size
    // of the allocation.
    function shortenThroughFmp(bytes memory s) external returns (bytes memory) {
        emit E(s);
        assembly {
            let size := add(and(add(mload(s), 0x1f), not(0x1f)), 0x20)
            mstore(sub(mload(0x40), size), 5)
        }
        return s;
    }

    // The free memory pointer moves back onto `s`, and the next allocation zeroes it.
    function rewind(bytes memory s) external returns (bytes memory) {
        emit E(s);
        assembly {
            mstore(0x40, s)
        }
        bytes memory t = new bytes(64);
        t;
        return s;
    }
}
