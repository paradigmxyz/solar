//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/boolean.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/bool_constant.sol

bool constant B = false;

contract BoolLiteral layout at true {} //~ ERROR: base slot of storage layout must evaluate to an integer
contract BoolConstant layout at B {} //~ ERROR: base slot of storage layout must evaluate to an integer
