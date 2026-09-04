// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/layout_specification_underflow_value.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/negative_number.sol

uint constant U = 42;
int constant I = 42;

contract UnderflowSub layout at 0 - 1 {} //~ ERROR: base slot of storage layout evaluates to a value outside
contract UnderflowExpression layout at 2**8 - 2**16 {} //~ ERROR: base slot of storage layout evaluates to a value outside
contract NegativeNumber layout at -1 {} //~ ERROR: base slot of storage layout evaluates to a value outside
contract NegativeSignedConstant layout at -I {} //~ ERROR: base slot of storage layout evaluates to a value outside
// Negating an unsigned constant is not arithmetic that underflows: the operator
// does not apply to the type, which is what solc reports here as well.
contract NegativeUnsignedConstant layout at -U {} //~ ERROR: cannot apply unary operator `-` to an unsigned type
