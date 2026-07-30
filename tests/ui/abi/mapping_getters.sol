//@ compile-flags: --emit=abi,hashes --pretty-json
//@ filecheck:

// CHECK-LABEL: "ROOT/tests/ui/abi/mapping_getters.sol:C":
// CHECK: "name": "data1"
// CHECK: "name": "data2"
// CHECK: "name": "nestedMapArray"
// CHECK: "hashes": {
// CHECK: "data1(uint256,bool,uint256)": "0a42c96e"
// CHECK: "data2(uint256,bool)": "23a808ad"
// CHECK: "nestedMapArray(uint256,uint256,bool,uint256,address,uint256)": "5d46ce82"

contract C {
    struct Data {
        uint a;
        bytes3 b;
        uint[3] c;
        uint[] d;
        bytes e;
    }
    mapping(uint => mapping(bool => Data[])) public data1;
    mapping(uint => mapping(bool => Data)) public data2;
    
    mapping(bool => mapping(address => uint256[])[])[][] public nestedMapArray;
}
