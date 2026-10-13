// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/assembly/for_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal pure returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        assembly {
            for {} eq(0,0) { c.slot := s.slot } {}
        }
    }
    function g() internal pure returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        assembly {
            for {} eq(0,1) { c.slot := s.slot } {}
        }
    }
    function h() internal pure returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        assembly {
            for {} eq(0,0) {} { c.slot := s.slot }
        }
    }
    function i() internal pure returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        assembly {
            for {} eq(0,1) {} { c.slot := s.slot }
        }
    }
}
