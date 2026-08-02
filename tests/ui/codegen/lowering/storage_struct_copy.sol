//@ run-call: readCopy => 0x0000000000000000000000000000000000001234, 0x12345678

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
}
