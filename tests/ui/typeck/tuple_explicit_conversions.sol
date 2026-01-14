//@compile-flags: -Ztypeck

contract TupleTests {
    // Valid: tuple element-wise explicit conversions.
    function testTupleConversion() public pure {
        (int8 a, int16 b) = (int8(1), int16(2));
        (int256 c, int256 d) = (int256(a), int256(b));
    }

    // Valid: nested tuple conversion.
    function testNestedTypes(uint8 a, uint16 b, uint32 c) public pure {
        (uint256 x, uint256 y, uint256 z) = (uint256(a), uint256(b), uint256(c));
    }
}

contract TupleErrors {
    // Invalid: mismatched tuple lengths.
    function testTupleLengthMismatch() public pure {
        (int8 a, int16 b, int32 c) = (int8(1), int16(2), int32(3));
        (int256 d, int256 e) = (int256(a), int256(b), int256(c)); //~ ERROR: mismatched number of components
    }
}
