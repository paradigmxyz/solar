// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/ternary_declaration_fine.sol
contract C {
    struct S { bool f; }
    S s;
    function f(bool flag) internal view {
        S storage c;
        flag ? c = s : c = s;
        c;
    }
    function g(bool flag) internal view {
        S storage c;
        flag ? c = s : (c = s);
        c;
    }
    function h(bool flag) internal view {
        S storage c;
        flag ? (c = s).f : (c = s).f;
        c;
    }
}
