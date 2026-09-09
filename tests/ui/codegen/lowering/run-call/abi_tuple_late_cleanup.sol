//@ codegen-matrix: standard
//@ run-call: probe 0, false => 0, 0, 0x1234, 1, 4, true
//@ run-call: probe 254, false => 254, -2, 0x1234, 1, 4, true
//@ run-call: probe 511, false => 255, -1, 0x1234, 1, 4, true
//@ run-call-fail: probe 511, true => Panic(0x21)

// The early dynamic member selects dynamic tuple encoding, followed by dirty
// scalar fields needing cleanup. Both byte fields alias the initialized source tail;
// mutating that tail after encoding must not change either decoded byte field.
contract AbiTupleLateCleanup {
    enum Mode {
        Off,
        On
    }

    struct Payload {
        bytes head;
        uint8 small;
        int8 signedWord;
        bytes2 pair;
        Mode mode;
        bytes mirror;
    }

    function probe(uint256 raw, bool bad) external pure returns (
        uint256 small, int256 signedWord, bytes2 pair, uint256 mode, uint256 length, bool intact
    ) {
        bytes memory source = hex"01020304";
        Payload memory value = Payload(source, 0, 0, 0x0000, Mode.Off, source);
        // Only declared scalar field words are dirtied. The object pointer,
        // dynamic-member pointers, length words and free-memory pointer stay valid.
        assembly {
            mstore(add(value, 0x20), raw)
            mstore(add(value, 0x40), raw)
            mstore(add(value, 0x60), 0x1234567800000000000000000000000000000000000000000000000000000000)
            mstore(add(value, 0x80), add(1, mul(bad, 4)))
        }
        bytes memory encoded = abi.encode(value);
        source[0] = 0xff;
        Payload memory decoded = abi.decode(encoded, (Payload));
        intact = value.mirror[0] == 0xff
            && keccak256(decoded.head) == keccak256(hex"01020304")
            && keccak256(decoded.mirror) == keccak256(hex"01020304");
        return (
            decoded.small,
            decoded.signedWord,
            decoded.pair,
            uint256(decoded.mode),
            decoded.head.length,
            intact
        );
    }
}
