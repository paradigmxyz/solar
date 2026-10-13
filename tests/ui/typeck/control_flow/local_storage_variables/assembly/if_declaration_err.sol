// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/assembly/if_declaration_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f(bool flag) internal pure {
        S storage c;
        assembly {
            if flag { c.slot := s.slot }
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
