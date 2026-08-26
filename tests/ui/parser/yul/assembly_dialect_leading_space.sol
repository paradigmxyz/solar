// ported-from: test/libsolidity/syntaxTests/inlineAssembly/assembly_dialect_leading_space.sol

function f() pure {
    assembly " evmasm" {} //~ ERROR: `evmasm` is the only supported assembly dialect
}
