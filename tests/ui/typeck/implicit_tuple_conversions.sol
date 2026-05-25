//@compile-flags: -Ztypeck

// Tests for implicit tuple conversions.
// Tuple assignments check element-wise type matching.

contract C {
    // === Valid: tuple with same types ===
    function tupleSameType(uint256 a, uint256 b) public pure {
        (uint256 x, uint256 y) = (a, b);
        (b, a) = (a, b);
    }

    function tupleSameType3(uint256 a, uint256 b, uint256 c) public pure {
        (uint256 x, uint256 y, uint256 z) = (a, b, c);
    }

    function tupleConditionalCommonType(address payable a, uint8 b, address c, uint256 d, bool cond) public pure {
        (address x, uint256 y) = cond ? (a, b) : (c, d);
    }

    // === Invalid: tuple type mismatch ===
    function tupleTypeMismatch(uint256 a, address b) public pure {
        (address x, uint256 y) = (
            a, //~ ERROR: mismatched types
            b  //~ ERROR: mismatched types
        );
    }

    // === Invalid: tuple element signedness mismatch (same width) ===
    function tupleSignednessMismatch(uint256 a, int256 b) public pure {
        (int256 x, uint256 y) = (
            a, //~ ERROR: mismatched types
            b  //~ ERROR: mismatched types
        );
    }

    // === Invalid: different element types ===
    function tupleDifferentTypes(uint256 a, bool b) public pure {
        (bool x, bool y) = (
            a, //~ ERROR: mismatched types
            b
        );
    }

    // === Invalid: tuple length mismatch ===
    function tupleLengthMismatch(uint256 a, uint256 b) public pure {
        (uint256 x, uint256 y, uint256 z) = (a, b); //~ ERROR: mismatched number of components
    }

    // === Invalid: no common tuple type ===
    function tupleConditionalNoCommonType(address payable a, uint256 b, address c, uint8 d, bool cond) public pure {
        cond ? (a, b) : (c, d); //~ ERROR: incompatible conditional types
    }
}
