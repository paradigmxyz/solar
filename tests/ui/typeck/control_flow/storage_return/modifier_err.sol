// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/modifier_err.sol
contract C {
    modifier callAndRevert() {
        _;
        revert();
    }
    modifier ifFlag(bool flag) {
        if (flag)
            _;
    }
    struct S { uint a; }
    S s;
    function f(bool flag) ifFlag(flag) internal view returns(S storage) { //~ ERROR: this variable is of storage pointer type and can be returned
        return s;
    }

    function g(bool flag) ifFlag(flag) callAndRevert() internal view returns(S storage) { //~ ERROR: this variable is of storage pointer type and can be returned
        return s;
    }
}
