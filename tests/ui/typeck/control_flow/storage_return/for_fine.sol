// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/for_fine.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view returns (S storage c) {
        for(c = s;;) {
        }
    }
    function g() internal view returns (S storage c) {
        for(; (c = s).f;) {
        }
    }
}
