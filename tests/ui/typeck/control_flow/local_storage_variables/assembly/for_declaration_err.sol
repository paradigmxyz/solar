// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/assembly/for_declaration_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal pure {
        S storage c;
        assembly {
            for {} eq(0,0) { c.slot := s.slot } {}
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function g() internal pure {
        S storage c;
        assembly {
            for {} eq(0,1) { c.slot := s.slot } {}
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function h() internal pure {
        S storage c;
        assembly {
            for {} eq(0,0) {} { c.slot := s.slot }
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function i() internal pure {
        S storage c;
        assembly {
            for {} eq(0,1) {} { c.slot := s.slot }
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
