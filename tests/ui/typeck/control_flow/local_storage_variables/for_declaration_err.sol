// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/for_declaration_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view {
        S storage c;
        for(;; c = s) {
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function g() internal view {
        S storage c;
        for(;;) {
            c = s;
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
