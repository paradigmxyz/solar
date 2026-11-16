// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/318_invalid_array_declaration_with_rational.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/array_length_fractional_computed.sol
// ported-from: test/libsolidity/syntaxTests/types/event_with_rational_size_array.sol

// solc also reports the missing data location, which the invalid type hides here.
contract RationalLength {
    function f() public {
        uint[3.5] a; a; //~ ERROR: array length cannot be fractional
    }
}

contract ComputedLength {
    uint constant a = 7;
    uint constant b = 3;
    function f() public {
        uint[a / b] memory x; x[0];
        uint[7 / 3] memory y; y[0]; //~ ERROR: array length cannot be fractional
    }
}

contract EventLength { event b(uint[(1 / 1)]); }

contract StateLengths {
    uint[0.5 * 4] integral;
    uint[-0.5] negativeFraction; //~ ERROR: array length cannot be fractional
    uint[(0.5)] parenthesized; //~ ERROR: array length cannot be fractional
    function f() public pure {
        new uint[2][](0.5); //~ ERROR: mismatched types
        uint[1.5][] memory nested; //~ ERROR: array length cannot be fractional
    }
}
