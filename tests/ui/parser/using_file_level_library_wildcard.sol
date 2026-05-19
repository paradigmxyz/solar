// Ported from test/libsolidity/syntaxTests/using/using_library_ast_file_level.sol.

library L {}

using L for *; //~ ERROR: the type has to be specified explicitly at file level
