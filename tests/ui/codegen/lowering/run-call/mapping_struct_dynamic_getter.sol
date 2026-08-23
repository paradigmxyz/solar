//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: MappingDynamicGetter::read() => 7, 0x616263
//@ run-call: MappingDynamicGetter::m(uint256) 1 => 7, 0x616263

contract MappingDynamicGetter {
    struct Entry {
        uint256 value;
        bytes data;
    }

    mapping(uint256 => Entry) public m;

    constructor() {
        m[1].value = 7;
        m[1].data = "abc";
    }

    function read() external view returns (uint256, bytes memory) {
        return (m[1].value, m[1].data);
    }
}
