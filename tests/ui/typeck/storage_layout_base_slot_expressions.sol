//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_binary_expression.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_constant_in_expression.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_max_value.sol

uint constant X = 42;

contract BinaryExpression layout at 0xffff * (0x123 + 0xABC) {}
contract ConstantInExpression layout at 0xffff * (50 - X) {}
contract MaxValue layout at 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF {}
