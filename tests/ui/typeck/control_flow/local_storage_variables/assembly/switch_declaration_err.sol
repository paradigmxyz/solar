// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/assembly/switch_declaration_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f(uint256 a) internal pure {
        S storage c;
        assembly {
            switch a
            case 0 { c.slot := s.slot }
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function g(bool flag) internal pure {
        S storage c;
        assembly {
            switch flag
            case 0 { c.slot := s.slot }
            case 1 { c.slot := s.slot }
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function h(uint256 a) internal pure {
        S storage c;
        assembly {
            switch a
            case 0 { c.slot := s.slot }
            default { return(0,0) }
        }
        c;
    }
}
