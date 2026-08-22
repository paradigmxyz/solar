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
//@[none, gas, size] run-call: exercise() => 0x05, 0x1234, 0x05, 0x1234, 0x05, 0x1234, 0x05, 0x05, 0x05
//@[none, gas, size] run-call: throughModifier() => 0x05

library FixedBytesLiteralLibrary {
    function identity(bytes2 value) internal pure returns (bytes2) {
        return value;
    }
}

contract FixedBytesLiteralBase {
    bytes2 internal baseValue;

    constructor(bytes2 value) {
        baseValue = value;
    }
}

contract FixedBytesLiteralContexts is FixedBytesLiteralBase(0x1234) {
    using FixedBytesLiteralLibrary for bytes2;

    struct Pair {
        bytes1 one;
        bytes2 two;
    }

    bytes1 private initialized = 0x05;
    bytes1 private modifierValue;
    bytes private byteValues;
    bytes1[] private fixedValues;

    modifier record(bytes1 value) {
        modifierValue = value;
        _;
    }

    function exercise()
        external
        returns (bytes1, bytes2, bytes1, bytes2, bytes1, bytes2, bytes1, bytes1, bytes1)
    {
        Pair memory pair = Pair(0x05, 0x1234);
        byteValues.push(0x05);
        fixedValues.push(0x05);
        return (
            initialized,
            baseValue,
            internalIdentity(0x05),
            bytes2(0x1234).identity(),
            pair.one,
            pair.two,
            this.externalIdentity(0x05),
            byteValues[0],
            fixedValues[0]
        );
    }

    function throughModifier() external record(0x05) returns (bytes1) {
        return modifierValue;
    }

    function externalIdentity(bytes1 value) external pure returns (bytes1) {
        return value;
    }

    function internalIdentity(bytes1 value) internal pure returns (bytes1) {
        return value;
    }
}
