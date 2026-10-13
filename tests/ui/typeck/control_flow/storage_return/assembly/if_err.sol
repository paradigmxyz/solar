// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/assembly/if_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f(bool flag) internal pure returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        assembly {
            if flag { c.slot := s.slot }
        }
    }
}
