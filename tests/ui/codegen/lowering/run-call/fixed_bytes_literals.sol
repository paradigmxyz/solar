//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: equal 0x01 => true
//@ run-call: equal 0x02 => false
//@ run-call: notEqual 0x01 => false
//@ run-call: notEqual 0x02 => true
//@ run-call: lessThan 0x7f => true
//@ run-call: lessThan 0x80 => false
//@ run-call: bitwiseOr 0x80 => 0x81
//@ run-call: wide 0x0102030405060708090a => true

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
