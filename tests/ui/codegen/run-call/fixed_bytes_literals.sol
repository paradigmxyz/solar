//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: equal(bytes1) 0x01 => true
//@ run-call: equal(bytes1) 0x02 => false
//@ run-call: notEqual(bytes1) 0x01 => false
//@ run-call: notEqual(bytes1) 0x02 => true
//@ run-call: lessThan(bytes1) 0x7f => true
//@ run-call: lessThan(bytes1) 0x80 => false
//@ run-call: bitwiseOr(bytes1) 0x80 => 0x81
//@ run-call: wide(bytes10) 0x0102030405060708090a => true

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
