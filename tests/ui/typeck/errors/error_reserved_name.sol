// ported-from: test/libsolidity/syntaxTests/errors/error_reserved_name.sol
error Error(uint); //~ ERROR: the built-in errors `Error` and `Panic` cannot be re-defined
