// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/intermediate_operation_out_of_range.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_overflow_value.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_bitwise_negation_literal.sol

contract IntermediateOperationOutOfRange layout at (2**256 + 1) * 2 - 2**256 - 3 {} //~ ERROR: failed to evaluate constant: arithmetic overflow
contract OverflowAdd layout at 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF + 1 {} //~ ERROR: failed to evaluate constant: arithmetic overflow
contract OverflowPow layout at 2**256 {} //~ ERROR: failed to evaluate constant: arithmetic overflow
contract BitwiseNegationLiteral layout at ~0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFE {} //~ ERROR: failed to evaluate constant: arithmetic overflow
