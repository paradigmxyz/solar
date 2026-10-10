// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/assembly/stub_declaration.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal pure {
        S storage c;
        assembly {
            c.slot := s.slot
        }
        c;
    }
}
