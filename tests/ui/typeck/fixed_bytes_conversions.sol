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
}
