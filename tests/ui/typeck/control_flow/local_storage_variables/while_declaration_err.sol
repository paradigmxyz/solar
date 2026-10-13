// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/while_declaration_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view {
        S storage c;
        while(false) {
            c = s;
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
