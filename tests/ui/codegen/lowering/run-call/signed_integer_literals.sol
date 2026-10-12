//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: negativeRemainder => -5
//@ run-call: mixedRemainder => -5
//@ run-call: negativeDivisorRemainder => 5
//@ run-call: negativeDivision => 2
//@ run-call: mixedDivision => -2
//@ run-call: rightShift 0 => -16
//@ run-call: rightShift 1 => -8
//@ run-call: rightShift 255 => -1
//@ run-call: rightShift 256 => -1
//@ run-call: leftShift 1 => -2
//@ run-call: leftShift 255 => -0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: leftShift 256 => 0
//@ run-call: lessThan => true
//@ run-call: greaterThan => false
//@ run-call: literalLessThan -10 => true
//@ run-call: literalLessThan -20 => false
//@ run-call: parameterLessThan -20 => true
//@ run-call: parameterLessThan -10 => false
//@ run-call: positiveRemainder => 5
//@ run-call: largePositiveDivision => 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: largePositiveRightShift 1 => 0x4000000000000000000000000000000000000000000000000000000000000000
//@ run-call: largePositiveGreaterThan => true

// Adapted from
// https://github.com/ethereum/solidity/blob/develop/test/libsolidity/semanticTests/operators/userDefined/operator_precendence.sol
contract SignedIntegerLiterals {
    function negativeRemainder() external pure returns (int256) {
        return -15 % -10;
    }

    function mixedRemainder() external pure returns (int256) {
        return -15 % 10;
    }

    function negativeDivisorRemainder() external pure returns (int256) {
        return 15 % -10;
    }

    function negativeDivision() external pure returns (int256) {
        return -20 / -10;
    }

    function mixedDivision() external pure returns (int256) {
        return -20 / 10;
    }

    function rightShift(uint256 shift) external pure returns (int256) {
        return -16 >> shift;
    }

    function leftShift(uint256 shift) external pure returns (int256) {
        return -1 << shift;
    }

    function lessThan() external pure returns (bool) {
        return -15 < -10;
    }

    function greaterThan() external pure returns (bool) {
        return -15 > -10;
    }

    function literalLessThan(int256 rhs) external pure returns (bool) {
        return -15 < rhs;
    }

    function parameterLessThan(int256 lhs) external pure returns (bool) {
        return lhs < -15;
    }

    function positiveRemainder() external pure returns (uint256) {
        return 15 % 10;
    }

    function largePositiveDivision() external pure returns (uint256) {
        return 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff / 2;
    }

    function largePositiveRightShift(uint256 shift) external pure returns (uint256) {
        return 0x8000000000000000000000000000000000000000000000000000000000000000 >> shift;
    }

    function largePositiveGreaterThan() external pure returns (bool) {
        return 0x8000000000000000000000000000000000000000000000000000000000000000 > 1;
    }
}
