// ported-from: test/libsolidity/syntaxTests/constants/initialization/constant_with_cyclic_dependency_2.sol
contract C {
    uint constant a = b * c; //~ ERROR: the value of the constant `a` has a cyclic dependency via `c`
    uint constant b = 7;
    uint constant c = b + uint(keccak256(abi.encodePacked(d))); //~ ERROR: the value of the constant `c` has a cyclic dependency via `d`
    uint constant d = 2 + a; //~ ERROR: the value of the constant `d` has a cyclic dependency via `a`
}
