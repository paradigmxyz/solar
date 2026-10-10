// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/if_declaration_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f(bool flag) internal {
        S storage c;
        if (flag) c = s;
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function g(bool flag) internal {
        S storage c;
        if (flag) c = s;
        else
        {
            if (!flag) c = s;
            else s.f = true;
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
