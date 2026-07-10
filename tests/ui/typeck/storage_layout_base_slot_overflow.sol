//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/intermediate_operation_out_of_range.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_overflow_value.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_bitwise_negation_literal.sol

contract IntermediateOperationOutOfRange layout at (2**256 + 1) * 2 - 2**256 - 3 {}
contract NegativeIntermediates layout at (2**2 - 2**3) * (2**5 - 2**8) {}
contract OverflowAdd layout at 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF + 1 {} //~ ERROR: outside the range of type `uint256`
contract OverflowPow layout at 2**256 {} //~ ERROR: outside the range of type `uint256`
contract BitwiseNegationLiteral layout at ~0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFE {} //~ ERROR: outside the range of type `uint256`
