//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: ConstructorFunctionCallFixedBytes::getName() => 0x616263
// ported-from: test/libsolidity/semanticTests/constructor/functions_called_by_constructor.sol

contract ConstructorFunctionCallFixedBytes {
    bytes3 internal name;

    constructor() {
        setName("abc");
    }

    function getName() external view returns (bytes3) {
        return name;
    }

    function setName(bytes3 name_) private {
        name = name_;
    }
}
