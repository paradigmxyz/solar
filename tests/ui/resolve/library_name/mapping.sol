// ported-from: test/libsolidity/syntaxTests/types/mapping/library_mapping.sol

library L {}
contract C
{
  mapping(bool => L) j; //~ ERROR: invalid use of a library name
  mapping(L => bool) i; //~ ERROR: invalid use of a library name
}
