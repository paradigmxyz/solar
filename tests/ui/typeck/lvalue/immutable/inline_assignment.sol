//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/immutable/variable_declaration_value.sol

contract C {
    int256 immutable x = x = 5;
}
