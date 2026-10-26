// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/203_struct_reference_compare_operators.sol

contract test {
  struct s {uint a;}
  s x;
  s y;
  fallback() external {
    x == y; //~ ERROR: cannot apply builtin operator `==`
  }
}
