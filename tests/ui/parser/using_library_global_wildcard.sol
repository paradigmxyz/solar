// Solc test: test/libsolidity/syntaxTests/using/global_library_with_asterisk.sol.

library L {}

using L for * global; //~ ERROR: the type has to be specified explicitly at file level
//~^ ERROR: can only globally attach functions to specific types
