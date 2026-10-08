// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/ternary_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f(bool flag) internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        flag ? (c = s).f : false;
    }
    function g(bool flag) internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        flag ? false : (c = s).f;
    }
}
