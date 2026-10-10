// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/assembly/reverting_function_declaration.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal pure {
        S storage c;
        // this could be allowed, but currently control flow for functions is not analysed
        assembly {
            function f() { revert(0, 0) }
            f()
        }
        c; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
