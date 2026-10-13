// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/default_location.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view returns (S memory c) {
        c = s;
    }
    function g() internal view returns (S memory) {
        return s;
    }
    function h() internal pure returns (S memory) {
    }
    function i(bool flag) internal view returns (S memory c) {
        if (flag) c = s;
    }
    function j(bool flag) internal view returns (S memory) { //~ WARN: unnamed return variable can remain unassigned
        if (flag) return s;
    }
}
