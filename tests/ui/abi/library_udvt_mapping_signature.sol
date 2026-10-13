//@ compile-flags: --emit=hashes --pretty-json
//@ filecheck:

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

    // CHECK-DAG: "key(mapping(A => uint256) storage)": "6f56faf4"
    function key(mapping(A => uint) storage) public {}
    // CHECK-DAG: "value(mapping(uint256 => A) storage)": "101cff9c"
    function value(mapping(uint => A) storage) public {}
    // CHECK-DAG: "arrayValue(mapping(uint256 => A[]) storage)": "5574097d"
    function arrayValue(mapping(uint => A[]) storage) public {}
    // CHECK-DAG: "fixedArrayValue(mapping(uint256 => A[2][]) storage)": "aff2eaa8"
    function fixedArrayValue(mapping(uint => A[2][]) storage) public {}
    // CHECK-DAG: "nested(mapping(A => mapping(A => uint256)) storage)": "535eecec"
    function nested(mapping(A => mapping(A => uint)) storage) public {}
    // CHECK-DAG: "named(mapping(L.E => L.S) storage)": "063a3ecf"
    function named(mapping(E => S) storage) public {}

    // CHECK-DAG: "plain(uint256)": "11546183"
    function plain(A) public {}
    // CHECK-DAG: "memoryArray(uint256[])": "45f70b77"
    function memoryArray(A[] memory) public {}
    // CHECK-DAG: "calldataArray(uint256[])": "1e12961c"
    function calldataArray(A[] calldata) external {}
    // CHECK-DAG: "structStorage(L.S storage)": "3c58540c"
    function structStorage(S storage) public {}

    // Distinct overloads with distinct selectors.
    // CHECK-DAG: "f(mapping(A => uint256) storage)": "14780190"
    function f(mapping(A => uint) storage) public {}
    // CHECK-DAG: "f(mapping(uint256 => uint256) storage)": "1d212bbe"
    function f(mapping(uint => uint) storage) public {}
}
