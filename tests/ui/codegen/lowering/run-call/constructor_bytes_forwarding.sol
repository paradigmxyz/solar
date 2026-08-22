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
//@[none] run-call: ConstructorBytesCreator::f(uint256,bytes) 1, 0x6162636465 => 1, 0x62
//@[gas] run-call: ConstructorBytesCreator::f(uint256,bytes) 1, 0x6162636465 => 1, 0x62
//@[size] run-call: ConstructorBytesCreator::f(uint256,bytes) 1, 0x6162636465 => 1, 0x62
// ported-from: test/libsolidity/semanticTests/constructor/bytes_in_constructors_packer.sol

contract ConstructorBytesBase {
    uint256 public value;
    bytes internal data;

    constructor(uint256 value_, bytes memory data_) {
        value = value_;
        data = data_;
    }

    function byteAt(uint256 index) external view returns (bytes1) {
        return data[index];
    }
}

contract ConstructorBytesMain is ConstructorBytesBase {
    constructor(bytes memory data_, uint256 value_)
        ConstructorBytesBase(value_, forward(data_))
    {}

    function forward(bytes memory data_) internal pure returns (bytes memory) {
        return data_;
    }
}

contract ConstructorBytesCreator {
    function f(uint256 index, bytes memory data)
        external
        returns (uint256 value, bytes1 selected)
    {
        ConstructorBytesMain created = new ConstructorBytesMain(data, index);
        value = created.value();
        selected = created.byteAt(index);
    }
}
