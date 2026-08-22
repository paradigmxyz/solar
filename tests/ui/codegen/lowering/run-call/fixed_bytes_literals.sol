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
//@[none] run-call: equal(bytes1) 0x01 => true
//@[gas] run-call: equal(bytes1) 0x01 => true
//@[size] run-call: equal(bytes1) 0x01 => true
//@[none] run-call: equal(bytes1) 0x02 => false
//@[gas] run-call: equal(bytes1) 0x02 => false
//@[size] run-call: equal(bytes1) 0x02 => false
//@[none] run-call: notEqual(bytes1) 0x01 => false
//@[gas] run-call: notEqual(bytes1) 0x01 => false
//@[size] run-call: notEqual(bytes1) 0x01 => false
//@[none] run-call: notEqual(bytes1) 0x02 => true
//@[gas] run-call: notEqual(bytes1) 0x02 => true
//@[size] run-call: notEqual(bytes1) 0x02 => true
//@[none] run-call: lessThan(bytes1) 0x7f => true
//@[gas] run-call: lessThan(bytes1) 0x7f => true
//@[size] run-call: lessThan(bytes1) 0x7f => true
//@[none] run-call: lessThan(bytes1) 0x80 => false
//@[gas] run-call: lessThan(bytes1) 0x80 => false
//@[size] run-call: lessThan(bytes1) 0x80 => false
//@[none] run-call: bitwiseOr(bytes1) 0x80 => 0x81
//@[gas] run-call: bitwiseOr(bytes1) 0x80 => 0x81
//@[size] run-call: bitwiseOr(bytes1) 0x80 => 0x81
//@[none] run-call: wide(bytes10) 0x0102030405060708090a => true
//@[gas] run-call: wide(bytes10) 0x0102030405060708090a => true
//@[size] run-call: wide(bytes10) 0x0102030405060708090a => true

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
