//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/revertStatement/error_used_elsewhere.sol

error E();

function f() pure {
    E(); //~ ERROR: errors can only be used with revert statements
}
