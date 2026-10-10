// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/321_rational_to_bytes_implicit_conversion.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/327_rational_index_access.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/344_one_divided_by_three_integer_conversion.sol
// ported-from: test/libsolidity/syntaxTests/enums/literal_conversion_error.sol
// ported-from: test/libsolidity/syntaxTests/constants/initialization/addmod_mulmod_rational_arg.sol

contract RationalToBytes {
    function f() public {
        bytes32 c = 3.2; c; //~ ERROR: mismatched types
    }
}

contract RationalIndexAccess {
    function f() public {
        uint[] memory a;
        a[.5]; //~ ERROR: mismatched types
    }
}

contract OneDividedByThree {
    function f() public {
        uint a = 1/3; //~ ERROR: mismatched types
    }
}

contract EnumConversion {
    enum Test { One, Two }
    function f() public {
        Test(-1); //~ ERROR: invalid explicit type conversion
        Test(2); //~ ERROR: invalid explicit type conversion
        Test(13); //~ ERROR: invalid explicit type conversion
        Test(5/3); //~ ERROR: invalid explicit type conversion
        Test(0.5); //~ ERROR: invalid explicit type conversion
    }
}

contract ModularArguments {
    uint constant a = addmod(3, 4, 0.1); //~ ERROR: mismatched types
    uint constant b = mulmod(3, 4, 0.1); //~ ERROR: mismatched types
}

// Fractions have no runtime type, so no explicit conversion applies either.
contract ExplicitConversions {
    enum E { A }
    function f() public pure {
        uint(0.5); //~ ERROR: invalid explicit type conversion
        int(-0.5); //~ ERROR: invalid explicit type conversion
        uint8(1 / 3); //~ ERROR: invalid explicit type conversion
        address(0.5); //~ ERROR: invalid explicit type conversion
        bytes1(0.5); //~ ERROR: invalid explicit type conversion
        E(0.5); //~ ERROR: invalid explicit type conversion
    }
}

// Literal arithmetic is exact, so integral results convert like any integer literal.
contract IntegralResults {
    function f() public pure {
        uint a = 2.5 * 2;
        uint8 b = 0.5 + 0.5 + 254;
        int8 c = -0.25 * 512;
        uint d = 1 / 3 * 3;
        uint e = 7 / 2 * 2;
        int g = (-1 / 2) ** -1;
        bytes1 h = 0.5 - 0.5;
        uint8 i = 0.5 + 0.5 + 255; //~ ERROR: mismatched types
        a; b; c; d; e; g; h; i;
    }
}
