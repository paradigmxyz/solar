//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_underflow_value.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/negative_number.sol

uint constant X = 42;

contract UnderflowSub layout at 0 - 1 {} //~ ERROR: base slot of storage layout evaluates to a value outside
contract UnderflowExpression layout at 2**8 - 2**16 {} //~ ERROR: base slot of storage layout evaluates to a value outside
contract NegativeNumber layout at -1 {} //~ ERROR: base slot of storage layout evaluates to a value outside
contract NegativeConstant layout at -X {} //~ ERROR: base slot of storage layout evaluates to a value outside
