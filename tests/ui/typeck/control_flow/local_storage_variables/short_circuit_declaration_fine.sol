// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/short_circuit_declaration_fine.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view {
        S storage c;
        (c = s).f && false;
        c;
    }
    function g() internal view {
        S storage c;
        (c = s).f || true;
        c;
    }
}
