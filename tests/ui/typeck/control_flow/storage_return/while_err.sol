// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/while_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        while(false) {
            c = s;
        }
    }
}
