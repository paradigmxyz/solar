//@ run-call: ConstructorBaseFixedBytes::value() => 0x616263

contract ConstructorBaseFixedBytesBase {
    bytes3 public value;

    constructor(bytes3 value_) {
        value = value_;
    }
}

contract ConstructorBaseFixedBytes is ConstructorBaseFixedBytesBase("abc") {}
