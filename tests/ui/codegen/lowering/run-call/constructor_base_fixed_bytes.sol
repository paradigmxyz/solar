//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: ConstructorBaseFixedBytes::value() => 0x616263

contract ConstructorBaseFixedBytesBase {
    bytes3 public value;

    constructor(bytes3 value_) {
        value = value_;
    }
}

contract ConstructorBaseFixedBytes is ConstructorBaseFixedBytesBase("abc") {}
