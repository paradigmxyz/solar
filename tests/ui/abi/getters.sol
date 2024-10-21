//@ignore-host: windows
//@compile-flags: --emit=abi,hashes --pretty-json

contract C {
    int public simple;
    int[] public array;
    mapping(int k => bool v) public map;
    mapping(int k => bool[] v) public mapOfArrays;
    mapping(int k => bool v)[] public arrayOfMaps;

    struct One {
        int x;
    }
    One public simpleOne;
    One[] public arrayOne;
    mapping(string k => One v) public mapOne;
    mapping(int k => One[] v) public mapOfArraysOne;
    mapping(int k => One v)[] public arrayOfMapsOne;

    struct Two {
        int x;
        bool y;
    }
    Two public simpleTwo;
    Two[] public arrayTwo;
    mapping(string k => Two v) public mapTwo;
    mapping(int k => Two[] v) public mapOfArraysTwo;
    mapping(int k => Two v)[] public arrayOfMapsTwo;
    
    struct RecMap {
        int x;
        mapping(int kn => Two[] vn) y;
        bool z;
    }
    RecMap public simpleRecMap;
    RecMap[] public arrayRecMap;
    mapping(string k => RecMap v) public mapRecMap;
    mapping(int k => RecMap[] v) public mapOfArraysRecMap;
    mapping(int k => RecMap v)[] public arrayOfMapsRecMap;
}
