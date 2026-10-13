//@ compile-flags: --emit=hashes --pretty-json

// solc prints `mapping` key and value types with their canonical names, so a
// user-defined value type keeps its name inside a `mapping` in a `library`
// function signature, but is unwrapped everywhere else.

type A is uint;

library L {
    struct S {
        A a;
    }

    enum E {
        X
    }

    function key(mapping(A => uint) storage) public {}
    function value(mapping(uint => A) storage) public {}
    function arrayValue(mapping(uint => A[]) storage) public {}
    function fixedArrayValue(mapping(uint => A[2][]) storage) public {}
    function nested(mapping(A => mapping(A => uint)) storage) public {}
    function named(mapping(E => S) storage) public {}

    function plain(A) public {}
    function memoryArray(A[] memory) public {}
    function calldataArray(A[] calldata) external {}
    function structStorage(S storage) public {}

    // Distinct overloads with distinct selectors.
    function f(mapping(A => uint) storage) public {}
    function f(mapping(uint => uint) storage) public {}
}
