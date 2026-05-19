//@compile-flags: -Ztypeck

contract C {
    // Valid: FixedBytes to FixedBytes (any size)
    function validBytesToBytes(bytes4 b4) public pure {
        bytes32 b32 = bytes32(b4);  // smaller to larger: right-pads with zeros
        bytes2 b2 = bytes2(b32);    // larger to smaller: truncates right
        bytes1 b1 = bytes1(b4);     // larger to smaller
        bytes16 b16 = bytes16(b1);  // smaller to larger
    }

    // Valid: FixedBytes to UInt (same size only)
    function validBytesToUint(bytes4 b4, bytes8 b8) public pure {
        uint32 u32 = uint32(b4);    // bytes4 (4 bytes) to uint32 (4 bytes)
        uint64 u64 = uint64(b8);    // bytes8 (8 bytes) to uint64 (8 bytes)
    }

    // Valid: UInt to FixedBytes (same size only)
    function validUintToBytes(uint32 u32) public pure {
        bytes4 b4 = bytes4(u32);    // uint32 (4 bytes) to bytes4 (4 bytes)
    }

    // Valid: same-size hex integer literals to FixedBytes.
    function validHexLiteralToBytes() public pure {
        bytes1 b1 = bytes1(0x01);
        bytes2 b2 = bytes2(0x0102);
        bytes4 b4 = bytes4(0x01ffc9a7);
        bytes32 b32 = bytes32(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff);
    }

    // Valid: same-size hex integer literals implicitly convert to FixedBytes.
    function validImplicitHexLiteralToBytes() public pure {
        bytes1 b1 = 0xff;
        bytes2 b2 = 0x0001;
        bytes4 b4 = 0x01ffc9a7;
        bytes32 b32 =
            (0x4b9f2d36e1b4c93de62cc077b00b1a91d84b6c31b4a14e012718dcca230689e7);
    }

    // Valid: zero integer literals to FixedBytes.
    function validZeroLiteralToBytes() public pure {
        bytes1 b1 = bytes1(0);
        bytes2 b2 = bytes2(0x00000);
        bytes32 b32 = bytes32(-0x0);
        bytes1 b3 = bytes1(1 - 1);
        bytes32 b4 = bytes32(0x01 - 0x01);
        bytes16 b16 = bytes16(0x00 + 0);
    }

    // Valid: zero integer literals implicitly convert to FixedBytes.
    function validImplicitZeroLiteralToBytes() public pure {
        bytes1 b1 = 0;
        bytes2 b2 = 0x00000;
        bytes32 b32 = -0x0;
        bytes1 b3 = 1 - 1;
        bytes32 b4 = 0x01 - 0x01;
        bytes16 b16 = 0x00 + 0;
    }

    // Invalid: FixedBytes to signed Int (not allowed)
    function invalidBytesToSignedInt(bytes4 b4, bytes8 b8) public pure {
        int32 i32 = int32(b4);      //~ ERROR: invalid explicit type conversion
        int64 i64 = int64(b8);      //~ ERROR: invalid explicit type conversion
    }

    // Invalid: signed Int to FixedBytes (not allowed)
    function invalidSignedIntToBytes(int64 i64) public pure {
        bytes8 b8 = bytes8(i64);    //~ ERROR: invalid explicit type conversion
    }

    // Invalid: FixedBytes to UInt (different size)
    function invalidBytesToUint(bytes4 b4) public pure {
        uint64 u64 = uint64(b4);    //~ ERROR: invalid explicit type conversion
        uint8 u8 = uint8(b4);       //~ ERROR: invalid explicit type conversion
    }

    // Invalid: UInt to FixedBytes (different size)
    function invalidUintToBytes(uint64 u64) public pure {
        bytes4 b4 = bytes4(u64);    //~ ERROR: invalid explicit type conversion
        bytes32 b32 = bytes32(u64); //~ ERROR: invalid explicit type conversion
    }

    // Invalid: non-zero integer literals to FixedBytes unless they are same-size hex literals.
    function invalidIntLiteralToBytes() public pure {
        bytes1 b1 = bytes1(1); //~ ERROR: invalid explicit type conversion
        bytes2 b2 = bytes2(256); //~ ERROR: invalid explicit type conversion
        bytes1 b3 = bytes1(-0x01); //~ ERROR: invalid explicit type conversion
        bytes1 b4 = bytes1(0x1); //~ ERROR: invalid explicit type conversion
        bytes2 b5 = bytes2(0xff); //~ ERROR: invalid explicit type conversion
        bytes1 b6 = bytes1(0x0100); //~ ERROR: invalid explicit type conversion
        bytes2 b7 = bytes2(0x010000); //~ ERROR: invalid explicit type conversion
        bytes1 b8 = bytes1(0x02 - 0x01); //~ ERROR: invalid explicit type conversion
        bytes1 b9 = bytes1(0x00 + 0x01); //~ ERROR: invalid explicit type conversion
        bytes2 b10 = bytes2(0x0102 + 0); //~ ERROR: invalid explicit type conversion
    }

    // Invalid: non-zero integer literals do not implicitly convert to FixedBytes unless they are same-size hex literals.
    function invalidImplicitIntLiteralToBytes() public pure {
        bytes1 b1 = 1; //~ ERROR: mismatched types
        bytes2 b2 = 256; //~ ERROR: mismatched types
        bytes1 b3 = -0x01; //~ ERROR: mismatched types
        bytes1 b4 = 0x1; //~ ERROR: mismatched types
        bytes2 b5 = 0xff; //~ ERROR: mismatched types
        bytes1 b6 = 0x0100; //~ ERROR: mismatched types
        bytes2 b7 = 0x010000; //~ ERROR: mismatched types
        bytes1 b8 = 0x02 - 0x01; //~ ERROR: mismatched types
        bytes1 b9 = 0x00 + 0x01; //~ ERROR: mismatched types
        bytes2 b10 = 0x0102 + 0; //~ ERROR: mismatched types
    }
}
