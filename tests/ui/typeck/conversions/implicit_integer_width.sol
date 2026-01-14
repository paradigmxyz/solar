//@compile-flags: -Ztypeck

// Tests for implicit integer width conversions.
// See: https://docs.soliditylang.org/en/latest/types.html#implicit-conversions
// "Smaller width to larger width is allowed (e.g., uint8 -> uint256, int8 -> int256)."

contract C {
    // === Valid: uint -> larger uint ===
    function validUintWidening(uint8 u8, uint16 u16, uint32 u32, uint128 u128) public pure {
        uint16 a = u8;
        uint32 b = u8;
        uint64 c = u8;
        uint128 d = u8;
        uint256 e = u8;

        uint32 f = u16;
        uint64 g = u16;
        uint128 h = u16;
        uint256 i = u16;

        uint64 j = u32;
        uint128 k = u32;
        uint256 l = u32;

        uint256 m = u128;
    }

    // === Valid: int -> larger int ===
    function validIntWidening(int8 i8, int16 i16, int32 i32, int128 i128) public pure {
        int16 a = i8;
        int32 b = i8;
        int64 c = i8;
        int128 d = i8;
        int256 e = i8;

        int32 f = i16;
        int64 g = i16;
        int128 h = i16;
        int256 i = i16;

        int64 j = i32;
        int128 k = i32;
        int256 l = i32;

        int256 m = i128;
    }

    // === Invalid: uint -> smaller uint (narrowing) ===
    function invalidUintNarrowing(uint16 u16, uint32 u32, uint256 u256) public pure {
        uint8 a = u16;   //~ ERROR: mismatched types
        uint8 b = u32;   //~ ERROR: mismatched types
        uint8 c = u256;  //~ ERROR: mismatched types
        uint16 d = u32;  //~ ERROR: mismatched types
        uint16 e = u256; //~ ERROR: mismatched types
        uint32 f = u256; //~ ERROR: mismatched types
    }

    // === Invalid: int -> smaller int (narrowing) ===
    function invalidIntNarrowing(int16 i16, int32 i32, int256 i256) public pure {
        int8 a = i16;   //~ ERROR: mismatched types
        int8 b = i32;   //~ ERROR: mismatched types
        int8 c = i256;  //~ ERROR: mismatched types
        int16 d = i32;  //~ ERROR: mismatched types
        int16 e = i256; //~ ERROR: mismatched types
        int32 f = i256; //~ ERROR: mismatched types
    }

    // === Invalid: uint <-> int (different signedness) ===
    function invalidSignednessChange(uint8 u8, uint256 u256, int8 i8, int256 i256) public pure {
        int8 a = u8;     //~ ERROR: mismatched types
        uint8 b = i8;    //~ ERROR: mismatched types
        int256 c = u256; //~ ERROR: mismatched types
        uint256 d = i256; //~ ERROR: mismatched types
        // Even larger size doesn't help
        int16 e = u8;    //~ ERROR: mismatched types
        uint16 f = i8;   //~ ERROR: mismatched types
    }

    // === Valid: same width assignments ===
    function validSameWidth(uint8 u8, uint16 u16, int8 i8, int16 i16) public pure {
        uint8 a = u8;
        uint16 b = u16;
        int8 c = i8;
        int16 d = i16;
    }
}
