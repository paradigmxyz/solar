// ported-from: test/libsolidity/syntaxTests/constants/initialization/constant_with_cyclic_dependency_library.sol
library A {
    uint256 constant VAL = B.VAL + 1; //~ ERROR: the value of the constant `VAL` has a cyclic dependency via `VAL`
}

library B {
    uint256 constant VAL = A.VAL + 1; //~ ERROR: the value of the constant `VAL` has a cyclic dependency via `VAL`
}
