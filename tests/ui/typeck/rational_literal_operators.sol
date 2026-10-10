// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/329_rational_as_exponent_value_signed.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/330_rational_as_exponent_value_unsigned.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/331_rational_as_exponent_half.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/332_rational_as_exponent_value_neg_quarter.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/372_shift_constant_right_fractional.sol
// ported-from: test/libsolidity/syntaxTests/types/rational_negative_numerator_negative_exp.sol

contract SignedExponent {
    function f() public {
        fixed g = 2 ** -2.2; //~ ERROR: cannot apply builtin operator `**` to `int_literal[2]` and `rational_literal`
    }
}

contract UnsignedExponent {
    function f() public {
        ufixed b = 3 ** 2.5; //~ ERROR: cannot apply builtin operator `**` to `int_literal[2]` and `rational_literal`
    }
}

contract HalfExponent {
    function f() public {
        2 ** (1/2); //~ ERROR: cannot apply builtin operator `**` to `int_literal[2]` and `rational_literal`
    }
}

contract NegativeQuarterExponent {
    function f() public {
        42 ** (-1/4); //~ ERROR: cannot apply builtin operator `**` to `int_literal[6]` and `rational_literal`
    }
}

contract ShiftByFraction {
    uint public a = 0x42 >> (1 / 2); //~ ERROR: cannot apply builtin operator `>>` to `int_literal[7]` and `rational_literal`
}

contract NegativeNumeratorNegativeExponent {
    function f() public pure returns (int) {
        return (-1 / 2) ** -1;
    }
}

// Fractions only support negation and arithmetic with other literals, with whole exponents, and
// comparisons with other fractions.
contract Operators {
    function f(uint x, int y) public pure {
        -0.5;
        ~0.5; //~ ERROR: cannot apply unary operator `~` to `rational_literal`
        !0.5; //~ ERROR: cannot apply unary operator `!` to `rational_literal`
        1.5 | 3; //~ ERROR: cannot apply builtin operator `|` to `rational_literal` and `int_literal[2]`
        1.5 & 3; //~ ERROR: cannot apply builtin operator `&` to `rational_literal` and `int_literal[2]`
        1.5 ^ 3; //~ ERROR: cannot apply builtin operator `^` to `rational_literal` and `int_literal[2]`
        1.5 << 1; //~ ERROR: cannot apply builtin operator `<<` to `rational_literal` and `int_literal[1]`
        1.5 >> 1; //~ ERROR: cannot apply builtin operator `>>` to `rational_literal` and `int_literal[1]`
        1 << 1.5; //~ ERROR: cannot apply builtin operator `<<` to `int_literal[1]` and `rational_literal`
        0.5 ** 2;
        0.5 ** -2;
        y ** 0.5; //~ ERROR: cannot apply builtin operator `**` to `int256` and `rational_literal`
        x + 0.5; //~ ERROR: cannot apply builtin operator `+` to `uint256` and `rational_literal`
        0.5 * x; //~ ERROR: cannot apply builtin operator `*` to `rational_literal` and `uint256`
        x % 0.5; //~ ERROR: cannot apply builtin operator `%` to `uint256` and `rational_literal`
        y - 0.5; //~ ERROR: cannot apply builtin operator `-` to `int256` and `rational_literal`
        x ** 0.5; //~ ERROR: cannot apply builtin operator `**` to `uint256` and `rational_literal`
        x << 0.5; //~ ERROR: cannot apply builtin operator `<<` to `uint256` and `rational_literal`
        0.5 && true; //~ ERROR: cannot apply builtin operator `&&` to `rational_literal` and `bool`
        0.3 < 0.5;
        0.5 == 0.5;
        0.5 != 0.7;
        0.5 < 1; //~ ERROR: cannot apply builtin operator `<` to `rational_literal` and `int_literal[1]`
        1 >= 0.5; //~ ERROR: cannot apply builtin operator `>=` to `int_literal[1]` and `rational_literal`
        0.5 == x; //~ ERROR: cannot apply builtin operator `==` to `rational_literal` and `uint256`
    }
}
