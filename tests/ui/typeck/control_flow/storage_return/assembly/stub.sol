// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/assembly/stub.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal pure returns (S storage c) {
        assembly {
            c.slot := s.slot
        }
    }
}
