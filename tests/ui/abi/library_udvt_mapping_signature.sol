//@ compile-flags: --emit=hashes --pretty-json
//@ filecheck:

// solc prints `mapping` key and value types with their canonical names, so a
// user-defined value type keeps its name inside a `mapping` in a `library`
// function signature, but is unwrapped everywhere else.

import {DA as Z} from "./auxiliary/udvt_mapping_types.sol";
import "./auxiliary/udvt_mapping_types.sol" as M;

type A is uint;

contract K {
    type KA is uint128;
}

library L {
    struct S {
        A a;
    }

    enum E {
        X
    }

    type LA is uint64;

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

    // Types keep the name and scope of their declaration, not of an import alias.
    // CHECK-DAG: "scoped(mapping(L.LA => K.KA) storage)": "74ad5794"
    function scoped(mapping(LA => K.KA) storage) public {}
    // CHECK-DAG: "aliased(mapping(DA => DL.DLA[]) storage)": "ea255aff"
    function aliased(mapping(Z => M.DL.DLA[]) storage) public {}

    // CHECK-DAG: "arrayOfMappings(mapping(A => uint256)[2] storage)": "3c6da3c1"
    function arrayOfMappings(mapping(A => uint)[2] storage) public {}
    // CHECK-DAG: "mappingArrayValue(mapping(uint256 => mapping(A => uint256)[]) storage)": "62559122"
    function mappingArrayValue(mapping(uint => mapping(A => uint)[]) storage) public {}
    // CHECK-DAG: "fnValue(mapping(A => function) storage)": "e8f1315b"
    function fnValue(mapping(A => function(A) external returns (A)) storage) public {}
}
