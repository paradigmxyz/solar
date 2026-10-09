// ported-from: test/libsolidity/syntaxTests/types/var_type_invalid_rational.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/112_exp_operator_exponent_too_big.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/235_abi_encode_with_large_integer_constant.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/533_tuple_invalid_literal_too_large_exp.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/534_tuple_invalid_literal_too_large_expression.sol
// ported-from: test/libsolidity/syntaxTests/indexing/array_multidim_rational.sol

contract InvalidRational {
    function f() internal pure {
        uint i = 31415999999999999999999999999999999999999999999999999999999999999999933**3; //~ ERROR: mismatched types
        uint unreachable = 123;
    }
}

contract ExponentTooBig {
    function f() public returns (uint d) { return 2 ** 10000000000; } //~ ERROR: failed to evaluate constant: arithmetic overflow
}

contract AbiEncodeLargeInteger {
    function f() pure public { abi.encode(2**500); } //~ ERROR: literal is too large for any integer type
}

contract TupleLiteralTooLargeExp {
    function f() pure public {
        (2**270, 1); //~ ERROR: literal is too large for any integer type
    }
}

contract TupleLiteralTooLargeExpression {
    function f() pure public {
        ((2**270) / 2**100, 1);
    }
}

contract MultidimRationalIndex {
  function f() public {
    bytes[32] memory a;
    a[8**90][8**90][8**90*0.1]; //~ ERROR: mismatched types
    //~^ ERROR: mismatched types
    //~^^ ERROR: mismatched types
  }
}

// Like solc, literal arithmetic can produce integers too large for any integer type. They only
// combine with other literals, and convert to no type.
contract Uses {
    function f(uint x, bytes32 b) public pure {
        uint8 a = 2**300; //~ ERROR: mismatched types
        uint8 c = 3 << 300; //~ ERROR: mismatched types
        int d = -(2**256); //~ ERROR: mismatched types
        uint e = 2**255 * 4; //~ ERROR: mismatched types
        bool g = 2**256 == 0; //~ ERROR: cannot apply builtin operator `==` to `int_literal[257]` and `int_literal[1]`
        x + 2**300; //~ ERROR: cannot apply builtin operator `+` to `uint256` and `int_literal[301]`
        x ** 2**300; //~ ERROR: cannot apply builtin operator `**` to `uint256` and `int_literal[301]`
        x << 2**300; //~ ERROR: cannot apply builtin operator `<<` to `uint256` and `int_literal[301]`
        (2**300) << x; //~ ERROR: cannot apply builtin operator `<<` to `int_literal[301]` and `uint256`
        b << 2**300; //~ ERROR: cannot apply builtin operator `<<` to `bytes32` and `int_literal[301]`
        2**300; //~ ERROR: literal is too large for any integer type
        (2**300); //~ ERROR: literal is too large for any integer type
        abi.encodePacked(2**300); //~ ERROR: literal is too large for any integer type
        uint h = true ? 2**300 : 1; //~ ERROR: invalid true type
        uint[2] memory i = [1, 2**300]; //~ ERROR: invalid mobile type
        a; c; d; e; g; h; i;
    }

    function exact() public pure {
        uint a = (1 << 300) / 3 * 3 >> 290;
        uint b = (1 << 300) * 0.5 >> 290;
        uint c = ((1 << 4095) * 2 * 1.5) >> 4095;
        int d = ~(2**300) + 2**300;
        int e = -(2**256) / 2**255;
        uint f = (2**256 + 1) * 2 - 2**256 - 3;
        a; b; c; d; e; f;
    }
}

// solc gives a fraction a fixed-point mobile type, which a fraction beyond 256 integer bits does
// not have.
contract FractionsTooLarge {
    function f() public pure {
        2**256 + 0.5; //~ ERROR: literal is too large for any fixed-point type
        (1, 2**256 - 0.5); //~ ERROR: literal is too large for any fixed-point type
        (1, -(2**255) - 0.5); //~ ERROR: literal is too large for any fixed-point type
        (1, 2**256 - 1.5);
        (1, -(2**255) + 0.5);
    }
}
