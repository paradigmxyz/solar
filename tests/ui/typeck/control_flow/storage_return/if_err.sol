// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/if_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f(bool flag) internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        if (flag) c = s;
    }
    function g(bool flag) internal returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        if (flag) c = s;
        else
        {
            if (!flag) c = s;
            else s.f = true;
        }
    }
}
