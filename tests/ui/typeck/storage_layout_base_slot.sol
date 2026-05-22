//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/simple_layout.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/literal_with_underscore.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/rational_number_without_fractional_part.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_binary_expression.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_constant_in_expression.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/intermediate_operation_out_of_range.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_max_value.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_overflow_value.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_underflow_value.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/negative_number.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_bitwise_negation_literal.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/boolean.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/bool_constant.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/hex_address.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/string.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/hex_string.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specified_by_attached_library_function.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_fractional_number.sol

uint constant X = 42;
bool constant B = true;

library L {
    function f(uint x) public pure returns (uint) {
        return x * 2;
    }
}

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
contract IntermediateOperationOutOfRange layout at (2**256 + 1) * 2 - 2**256 - 3 {}
contract MaxValue layout at 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF {}

contract AttachedFunction layout at 2.f() { //~ ERROR: base slot of storage layout must be a compile-time constant expression
    using L for *;
}

contract OverflowAdd layout at 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF + 1 {} //~ ERROR: base slot of storage layout evaluates to
contract OverflowPow layout at 2**256 {} //~ ERROR: base slot of storage layout evaluates to
contract UnderflowSub layout at 0 - 1 {} //~ ERROR: base slot of storage layout evaluates to
contract UnderflowExpression layout at 2**8 - 2**16 {} //~ ERROR: base slot of storage layout evaluates to
contract NegativeNumber layout at -1 {} //~ ERROR: base slot of storage layout evaluates to
contract NegativeConstant layout at -X {} //~ ERROR: base slot of storage layout evaluates to
contract BitwiseNegationLiteral layout at ~0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFE {} //~ ERROR: base slot of storage layout evaluates to

contract BoolLiteral layout at true {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract BoolConstant layout at B {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract HexAddress layout at 0xdCad3a6d3569DF655070DEd06cb7A1b2Ccd1D3AF {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract StringLiteral layout at "MyLayoutBase" {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract HexStringLiteral layout at hex"616263" {} //~ ERROR: base slot of storage layout must evaluate to an integer

contract FractionalDivision layout at 3/2 {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract FractionalNumber layout at 4.2 {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract LeadingFractionalNumber layout at .1 {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract NegativeExponent layout at 42e-10 {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract UnderscoredNegativeExponent layout at 1_7e-10 {} //~ ERROR: base slot of storage layout must evaluate to an integer
