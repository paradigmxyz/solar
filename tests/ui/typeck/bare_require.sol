// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/432_bare_require.sol

contract C {
    // This is different because it does have overloads.
    function f() pure public { require; } //~ ERROR: no matching declarations found
}
