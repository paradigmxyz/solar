//@compile-flags: -Ztypeck

contract IntegerConversions {
    // Int <-> Int (any size - only width changes)
    function validIntToInt(int8 i8, int256 i256) public pure {
        int256 a = int256(i8);
        int8 b = int8(i256);
        int128 c = int128(i8);
        int64 d = int64(i8);
        int16 e = int16(i256);
    }

    // UInt <-> UInt (any size - only width changes)
    function validUintToUint(uint8 u8, uint256 u256) public pure {
        uint256 a = uint256(u8);
        uint8 b = uint8(u256);
        uint128 c = uint128(u8);
        uint64 d = uint64(u8);
        uint16 e = uint16(u256);
    }

    // Int <-> UInt (same size only)
    function validIntUintSameSize(int8 i8, uint8 u8, int128 i128, uint128 u128) public pure {
        uint8 a = uint8(i8);
        int8 b = int8(u8);
        uint128 c = uint128(i128);
        int128 d = int128(u128);
    }

    // Int <-> UInt (different sizes - multi-aspect conversion)
    // Cannot change both sign and width in one conversion
    function invalidIntUintDifferentSize(int8 i8, uint256 u256, int16 i16, uint32 u32) public pure {
        uint256 a = uint256(i8);   //~ ERROR: invalid explicit type conversion
        int8 b = int8(u256);       //~ ERROR: invalid explicit type conversion
        uint32 c = uint32(i16);    //~ ERROR: invalid explicit type conversion
        int16 d = int16(u32);      //~ ERROR: invalid explicit type conversion
        uint128 e = uint128(i8);   //~ ERROR: invalid explicit type conversion
    }

    // IntLiteral (positive) -> Int/UInt (any size)
    function validPositiveLiteralToInt() public pure {
        uint8 a = uint8(42);
        uint16 b = uint16(256);
        uint256 c = uint256(12345);
        int8 d = int8(42);
        int16 e = int16(100);
        int256 f = int256(12345);
    }

    // Positive literals with various sizes
    function validPositiveLiteralVariousSizes() public pure {
        uint128 a = uint128(0);
        uint32 b = uint32(0xFFFFFFFF);
        int64 d = int64(9223372036854775807);  // max int64
        int64 e = int64(9223372036854775808);   //~ ERROR: invalid explicit type conversion
    }

    // IntLiteral (negative) -> Int
    // Note: unary minus on literals requires special handling in parser
    function validNegativeLiteralToInt() public pure {
        int8 a = int8(42);
        int16 b = int16(256);
        int256 c = int256(12345);
        int128 d = int128(1);
    }

    // IntLiteral -> IntLiteral (allowed)
    function validLiteralToLiteral() public pure {
        int256 a = int256(uint256(42));
        uint256 b = uint256(uint128(100));
        int64 c = int64(int32(42));
    }

    // UInt -> FixedBytes (same size)
    function validUintToBytes(uint8 u8, uint32 u32, uint256 u256) public pure {
        bytes1 b1 = bytes1(u8);
        bytes4 b4 = bytes4(u32);
        bytes32 b32 = bytes32(u256);
    }

    // UInt -> FixedBytes (different size)
    function invalidUintToBytesDifferentSize(uint8 u8, uint32 u32) public pure {
        bytes2 b2 = bytes2(u8);      //~ ERROR: invalid explicit type conversion
        bytes8 b8 = bytes8(u32);     //~ ERROR: invalid explicit type conversion
    }

    // Nested type conversions
    function validComplexConversions() public pure {
        uint256 a = uint256(int256(42));
        int128 b = int128(int64(100));
        uint8 c = uint8(uint16(255));
    }

    function validNestedConversions(int8 i8) public pure {
        uint16 a = uint16(int16(i8));
        uint32 b = uint32(int32(i8));
    }
}
