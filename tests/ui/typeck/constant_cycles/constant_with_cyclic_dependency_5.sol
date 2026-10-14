// ported-from: test/libsolidity/syntaxTests/constants/initialization/constant_with_cyclic_dependency_5.sol
contract C {
  uint constant a = uint(keccak256(abi.encode(d))); //~ ERROR: the value of the constant `a` has a cyclic dependency via `d`
  uint c = uint(keccak256(abi.encode(d)));
  uint constant d = a; //~ ERROR: the value of the constant `d` has a cyclic dependency via `a`
}
