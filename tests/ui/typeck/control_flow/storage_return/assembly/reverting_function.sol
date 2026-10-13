// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/assembly/reverting_function.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal pure returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        // this could be allowed, but currently control flow for functions is not analysed
        assembly {
            function f() { revert(0, 0) }
            f()
        }
    }
}
