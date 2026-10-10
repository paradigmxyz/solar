// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/short_circuit_declaration_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view {
        S storage c;
        false && (c = s).f;
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function g() internal view {
        S storage c;
        true || (c = s).f;
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function h() internal view {
        S storage c;
        // expect error, although this is always fine
        true && (false || (c = s).f);
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
