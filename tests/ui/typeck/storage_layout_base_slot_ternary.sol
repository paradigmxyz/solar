// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/ternary_operator.sol

uint constant T = true ? 42 : 94;

contract Ternary layout at true ? 42 : 94 {} //~ ERROR: base slot of storage layout cannot be evaluated at compile time
contract TernaryOperand layout at 255 + (true ? 1 : 0) {} //~ ERROR: base slot of storage layout cannot be evaluated at compile time
contract TernaryConstant layout at T {} //~ ERROR: base slot of storage layout cannot be evaluated at compile time
