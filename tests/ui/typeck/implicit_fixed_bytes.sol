//@compile-flags: -Ztypeck

// Tests for implicit FixedBytes width conversions.
// See: https://docs.soliditylang.org/en/latest/types.html#implicit-conversions
// "Smaller bytesN to larger bytesN is allowed (e.g., bytes1 -> bytes32, right-padded with zeros)."

contract C {
    // === Valid: smaller bytesN -> larger bytesN (right-padded with zeros) ===
    function validBytesWidening(bytes1 b1, bytes2 b2, bytes4 b4, bytes16 b16) public pure {
        bytes2 a = b1;
        bytes4 c = b1;
        bytes8 d = b1;
        bytes16 e = b1;
        bytes32 f = b1;

        bytes4 g = b2;
        bytes8 h = b2;
        bytes16 i = b2;
        bytes32 j = b2;

        bytes8 k = b4;
        bytes16 l = b4;
        bytes32 m = b4;

        bytes32 n = b16;
    }

    // === Invalid: larger bytesN -> smaller bytesN (truncation requires explicit cast) ===
    function invalidBytesNarrowing(bytes2 b2, bytes4 b4, bytes16 b16, bytes32 b32) public pure {
        bytes1 a = b2;   //~ ERROR: mismatched types
        bytes1 b = b4;   //~ ERROR: mismatched types
        bytes1 c = b16;  //~ ERROR: mismatched types
        bytes1 d = b32;  //~ ERROR: mismatched types
        bytes2 e = b4;   //~ ERROR: mismatched types
        bytes2 f = b32;  //~ ERROR: mismatched types
        bytes4 g = b16;  //~ ERROR: mismatched types
        bytes4 h = b32;  //~ ERROR: mismatched types
        bytes16 i = b32; //~ ERROR: mismatched types
    }

    // === Valid: same size assignments ===
    function validSameSize(bytes1 b1, bytes4 b4, bytes32 b32) public pure {
        bytes1 a = b1;
        bytes4 b = b4;
        bytes32 c = b32;
    }
}
