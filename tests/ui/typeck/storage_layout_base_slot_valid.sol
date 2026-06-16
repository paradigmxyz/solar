//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/simple_layout.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/literal_with_underscore.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/rational_number_without_fractional_part.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_binary_expression.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_constant_in_expression.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_max_value.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/hex_address.sol

uint constant X = 42;

contract SimpleHex layout at 0x1234 {}
contract SimpleDec layout at 1024 {}
contract SimpleZero layout at 0 {}

contract UnderscoreDecimalExponent layout at 42_0e10 {}
contract UnderscoreHex layout at 0x1234_ABCD {}
contract UnderscoreDecimal layout at 1234_000 {}

contract RationalWithoutFractionalPart layout at 42.0 {}
contract RationalExponentWithoutFractionalPart layout at 2.5e10 {}
contract RationalDivisionWithoutFractionalPart layout at 12/3 {}

contract BinaryExpression layout at 0xffff * (0x123 + 0xABC) {}
contract ConstantInExpression layout at 0xffff * (50 - X) {}
contract MaxValue layout at 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF {}

contract HexAddress layout at 0xdCad3a6d3569DF655070DEd06cb7A1b2Ccd1D3AF {}
