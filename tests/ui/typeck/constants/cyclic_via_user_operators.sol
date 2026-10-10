// ported-from: test/libsolidity/syntaxTests/constants/constant_cyclic_via_user_operators.sol
type Type is uint;
using {f as +} for Type global;
function f(Type, Type) pure returns (Type) {}

Type constant t = Type.wrap(1);
Type constant u = v + t; //~ ERROR: initial value for constant variable has to be compile-time constant
Type constant v = u + t; //~ ERROR: initial value for constant variable has to be compile-time constant
