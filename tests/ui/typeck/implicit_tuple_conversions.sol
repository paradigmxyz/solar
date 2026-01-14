//@compile-flags: -Ztypeck

// Tests for implicit conversions in tuple contexts.
// Tuple assignments check element-wise implicit conversions.

contract C {
    // === Valid: tuple assignment with widening ===
    function tupleWidening(uint8 a, uint8 b) public pure {
        (uint256 x, uint256 y) = (a, b); // implicit uint8 -> uint256
    }

    function tupleWideningInt(int8 a, int8 b) public pure {
        (int256 x, int256 y) = (a, b); // implicit int8 -> int256
    }

    function tupleWideningMixed(uint8 a, uint16 b, uint32 c) public pure {
        (uint256 x, uint256 y, uint256 z) = (a, b, c); // all widen to uint256
    }

    // === Valid: tuple with same types ===
    function tupleSameType(uint256 a, uint256 b) public pure {
        (uint256 x, uint256 y) = (a, b);
    }

    // === Invalid: tuple narrowing (larger -> smaller) ===
    function tupleNarrowingInvalid(uint256 a, uint256 b) public pure {
        (uint8 x, uint8 y) = (a, b); //~ ERROR: mismatched types
        //~^ ERROR: mismatched types
    }

    function tupleNarrowingIntInvalid(int256 a, int256 b) public pure {
        (int8 x, int8 y) = (a, b); //~ ERROR: mismatched types
        //~^ ERROR: mismatched types
    }

    // === Invalid: tuple signedness mismatch ===
    function tupleSignednessMismatch(uint8 a, int8 b) public pure {
        (int8 x, uint8 y) = (a, b); //~ ERROR: mismatched types
        //~^ ERROR: mismatched types
    }

    // === Valid: bytes widening in tuples ===
    function tupleBytesWidening(bytes1 a, bytes2 b) public pure {
        (bytes32 x, bytes32 y) = (a, b); // implicit widening
    }

    // === Invalid: bytes narrowing in tuples ===
    function tupleBytesNarrowingInvalid(bytes32 a, bytes32 b) public pure {
        (bytes1 x, bytes2 y) = (a, b); //~ ERROR: mismatched types
        //~^ ERROR: mismatched types
    }
}
