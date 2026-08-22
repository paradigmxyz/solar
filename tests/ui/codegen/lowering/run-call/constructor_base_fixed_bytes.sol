//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: ConstructorBaseFixedBytes::value() => 0x616263

contract ConstructorBaseFixedBytesBase {
    bytes3 public value;

    constructor(bytes3 value_) {
        value = value_;
    }
}

contract ConstructorBaseFixedBytes is ConstructorBaseFixedBytesBase("abc") {}
