//@ codegen-matrix: standard
//@ run-call: roundtrip (7, (11, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122, 13)) => (7, (11, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122, 13))
//@ run-call: roundtrip (7, (11, 0x, 13)) => (7, (11, 0x, 13))
//@ run-call: fields 5, (7, (11, 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122, 13)) => 0x00000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000007000000000000000000000000000000000000000000000000000000000000000b00000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000d00000000000000000000000000000000000000000000000000000000000000220102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122000000000000000000000000000000000000000000000000000000000000
//@ run-call: reset 0x0000000000000000000000000000000000000000000000000000000000000009 => 41
//@ run-call: reset 0x00000000000000000000000000000000000000000000000000000000000000050101010101010101010101010101010101010101010101010101010101010101 => 69
//@ run-call-fail: reset 0x => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

struct Inner {
    uint256 a;
    bytes data;
    uint256 b;
}

struct Deposit {
    uint256 id;
    Inner inner;
}

// A struct decoded from calldata with a `bytes` field is copied into an allocation whose extent
// is only known at run time, and the encoder then reads the fields back. The copy cannot reach
// the earlier allocations or the words of its own allocation below its start.
contract AllocUpward {
    function roundtrip(Deposit memory d) external pure returns (Deposit memory) {
        return d;
    }

    function fields(uint256 pre, Deposit memory d) external pure returns (bytes memory) {
        uint256[3] memory head = [pre, d.id, d.inner.a];
        return abi.encode(head, d.inner.data, d.inner.b);
    }

    // The assembly moves the free-memory pointer back, so the allocation of the copy overlaps `a`.
    function reset(bytes calldata data) external pure returns (uint256 r) {
        uint256[] memory a = new uint256[](1);
        a[0] = 1;
        assembly {
            mstore(0x40, a)
        }
        bytes memory b = data;
        r = a[0] + b.length;
    }
}
