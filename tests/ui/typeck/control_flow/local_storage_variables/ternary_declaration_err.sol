// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/ternary_declaration_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f(bool flag) internal view {
        S storage c;
        flag ? (c = s).f : false;
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function g(bool flag) internal view {
        S storage c;
        flag ? false : (c = s).f;
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
