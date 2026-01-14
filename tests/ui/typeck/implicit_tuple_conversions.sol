//@compile-flags: -Ztypeck

// Tests for implicit tuple conversions.
// Tuple assignments check element-wise type matching.

contract C {
    // === Valid: tuple with same types ===
    function tupleSameType(uint256 a, uint256 b) public pure {
        (uint256 x, uint256 y) = (a, b);
    }

    function tupleSameType3(uint256 a, uint256 b, uint256 c) public pure {
        (uint256 x, uint256 y, uint256 z) = (a, b, c);
    }

    // === Invalid: tuple type mismatch ===
    function tupleTypeMismatch(uint256 a, address b) public pure {
        (address x, uint256 y) = (a, b); //~ ERROR: mismatched types
        //~^ ERROR: mismatched types
    }

    // === Invalid: tuple element signedness mismatch (same width) ===
    function tupleSignednessMismatch(uint256 a, int256 b) public pure {
        (int256 x, uint256 y) = (a, b); //~ ERROR: mismatched types
        //~^ ERROR: mismatched types
    }

    // === Invalid: different element types ===
    function tupleDifferentTypes(uint256 a, uint256 b) public pure {
        (bool x, bool y) = (a, b); //~ ERROR: mismatched types
        //~^ ERROR: mismatched types
    }
}
