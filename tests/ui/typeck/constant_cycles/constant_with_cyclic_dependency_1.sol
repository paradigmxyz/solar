// ported-from: test/libsolidity/syntaxTests/constants/initialization/constant_with_cyclic_dependency_1.sol
contract C {
    uint constant a = a; //~ ERROR: the value of the constant `a` has a cyclic dependency via `a`
}
