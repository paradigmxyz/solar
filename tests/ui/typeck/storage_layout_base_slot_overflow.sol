// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/intermediate_operation_out_of_range.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_overflow_value.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_bitwise_negation_literal.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/contract_extends_past_storage_end.sol

contract IntermediateOperationOutOfRange layout at (2**256 + 1) * 2 - 2**256 - 3 {}
contract OverflowAdd layout at 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF + 1 {} //~ ERROR: base slot of storage layout evaluates to a value outside the range of type `uint256`
contract OverflowPow layout at 2**256 {} //~ ERROR: base slot of storage layout evaluates to a value outside the range of type `uint256`
contract BitwiseNegationLiteral layout at ~0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFE {} //~ ERROR: base slot of storage layout evaluates to a value outside the range of type `uint256`

contract ExtendsPastEnd layout at 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff {
    //~^ ERROR: contract extends past the end of storage when this base slot value is specified
    uint x;
}
