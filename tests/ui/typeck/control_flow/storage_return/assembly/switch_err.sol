// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/assembly/switch_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f(uint256 a) internal pure returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        assembly {
            switch a
            case 0 { c.slot := s.slot }
        }
    }
    function g(bool flag) internal pure returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        assembly {
            switch flag
            case 0 { c.slot := s.slot }
            case 1 { c.slot := s.slot }
        }
    }
    function h(uint256 a) internal pure returns (S storage c) {
        assembly {
            switch a
            case 0 { c.slot := s.slot }
            default { return(0,0) }
        }
    }
}
