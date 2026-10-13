// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/short_circuit_fine.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view returns (S storage c) {
        (c = s).f && false;
    }
    function g() internal view returns (S storage c) {
        (c = s).f || true;
    }
}
