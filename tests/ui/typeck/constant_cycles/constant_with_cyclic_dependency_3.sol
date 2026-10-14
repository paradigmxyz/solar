// ported-from: test/libsolidity/syntaxTests/constants/initialization/constant_with_cyclic_dependency_3.sol
contract C {
    uint constant x = a; //~ ERROR: the value of the constant `x` has a cyclic dependency via `a`
    uint constant a = b * c; //~ ERROR: the value of the constant `a` has a cyclic dependency via `b`
    uint constant b = c; //~ ERROR: the value of the constant `b` has a cyclic dependency via `c`
    uint constant c = b; //~ ERROR: the value of the constant `c` has a cyclic dependency via `b`
}
