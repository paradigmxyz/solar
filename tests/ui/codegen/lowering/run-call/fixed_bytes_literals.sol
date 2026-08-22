//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: equal(bytes1) 0x01 => true
//@[none, gas, size] run-call: equal(bytes1) 0x02 => false
//@[none, gas, size] run-call: notEqual(bytes1) 0x01 => false
//@[none, gas, size] run-call: notEqual(bytes1) 0x02 => true
//@[none, gas, size] run-call: lessThan(bytes1) 0x7f => true
//@[none, gas, size] run-call: lessThan(bytes1) 0x80 => false
//@[none, gas, size] run-call: bitwiseOr(bytes1) 0x80 => 0x81
//@[none, gas, size] run-call: wide(bytes10) 0x0102030405060708090a => true

contract FixedBytesLiterals {
    function equal(bytes1 value) external pure returns (bool) {
        return value == 0x01;
    }

    function notEqual(bytes1 value) external pure returns (bool) {
        return value != 0x01;
    }

    function lessThan(bytes1 value) external pure returns (bool) {
        return value < 0x80;
    }

    function bitwiseOr(bytes1 value) external pure returns (bytes1) {
        return value | 0x01;
    }

    function wide(bytes10 value) external pure returns (bool) {
        return value == 0x0102030405060708090a;
    }
}
