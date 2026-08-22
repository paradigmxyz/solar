//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: readCopy => 0x0000000000000000000000000000000000001234, 0x12345678
//@[gas] run-call: readCopy => 0x0000000000000000000000000000000000001234, 0x12345678
//@[size] run-call: readCopy => 0x0000000000000000000000000000000000001234, 0x12345678
//@[none] run-call: readDirect => 0x0000000000000000000000000000000000001234, 0x12345678
//@[gas] run-call: readDirect => 0x0000000000000000000000000000000000001234, 0x12345678
//@[size] run-call: readDirect => 0x0000000000000000000000000000000000001234, 0x12345678

contract StorageStructCopy {
    struct Value {
        address owner;
        bytes4[] selectors;
    }

    Value private value;

    constructor() {
        value.owner = address(0x1234);
        value.selectors.push(bytes4(0x12345678));
    }

    function readCopy() external view returns (address, bytes4) {
        Value storage source = value;
        Value memory result = source;
        return (result.owner, result.selectors[0]);
    }

    function readDirect() external view returns (address, bytes4) {
        Value memory result = value;
        return (result.owner, result.selectors[0]);
    }
}
