// ported-from: test/libsolidity/syntaxTests/errors/panic_reserved_name.sol
error Panic(bytes2); //~ ERROR: the built-in errors `Error` and `Panic` cannot be re-defined
