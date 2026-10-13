// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/while_declaration_fine.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view {
        S storage c;
        while((c = s).f) {
        }
        c;
    }
    function g() internal view {
        S storage c;
        c = s;
        while(false) {
        }
        c;
    }
    function h() internal view {
        S storage c;
        while(false) {
        }
        c = s;
        c;
    }
}
