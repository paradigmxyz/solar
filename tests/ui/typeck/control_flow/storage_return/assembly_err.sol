// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/assembly_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal pure returns (S storage) { //~ ERROR: this variable is of storage pointer type and can be returned
        assembly {
        }
    }
}
