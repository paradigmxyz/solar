// Solc test: test/libsolidity/syntaxTests/using/using_for_ast_file_level.sol.

function f(uint256) pure {}

using {f} for *; //~ ERROR: the type has to be specified explicitly at file level
