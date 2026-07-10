//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/simple_layout.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/literal_with_underscore.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/rational_number_without_fractional_part.sol

contract SimpleHex layout at 0x1234 {}
contract SimpleDec layout at 1024 {}
contract SimpleZero layout at 0 {}

contract UnderscoreDecimalExponent layout at 42_0e10 {}
contract UnderscoreHex layout at 0x1234_ABCD {}
contract UnderscoreDecimal layout at 1234_000 {}

contract RationalWithoutFractionalPart layout at 42.0 {}
contract RationalExponentWithoutFractionalPart layout at 2.5e10 {}
contract RationalDivisionWithoutFractionalPart layout at 12/3 {}
