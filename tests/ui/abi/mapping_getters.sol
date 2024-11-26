//@ignore-host: windows
//@compile-flags: --emit=abi,hashes --pretty-json

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
