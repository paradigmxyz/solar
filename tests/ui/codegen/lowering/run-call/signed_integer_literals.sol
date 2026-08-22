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
//@[none] run-call: negativeRemainder() => -5
//@[gas] run-call: negativeRemainder() => -5
//@[size] run-call: negativeRemainder() => -5
//@[none] run-call: mixedRemainder() => -5
//@[gas] run-call: mixedRemainder() => -5
//@[size] run-call: mixedRemainder() => -5
//@[none] run-call: negativeDivisorRemainder() => 5
//@[gas] run-call: negativeDivisorRemainder() => 5
//@[size] run-call: negativeDivisorRemainder() => 5
//@[none] run-call: negativeDivision() => 2
//@[gas] run-call: negativeDivision() => 2
//@[size] run-call: negativeDivision() => 2
//@[none] run-call: mixedDivision() => -2
//@[gas] run-call: mixedDivision() => -2
//@[size] run-call: mixedDivision() => -2
//@[none] run-call: rightShift(uint256) 0 => -16
//@[gas] run-call: rightShift(uint256) 0 => -16
//@[size] run-call: rightShift(uint256) 0 => -16
//@[none] run-call: rightShift(uint256) 1 => -8
//@[gas] run-call: rightShift(uint256) 1 => -8
//@[size] run-call: rightShift(uint256) 1 => -8
//@[none] run-call: rightShift(uint256) 255 => -1
//@[gas] run-call: rightShift(uint256) 255 => -1
//@[size] run-call: rightShift(uint256) 255 => -1
//@[none] run-call: rightShift(uint256) 256 => -1
//@[gas] run-call: rightShift(uint256) 256 => -1
//@[size] run-call: rightShift(uint256) 256 => -1
//@[none] run-call: leftShift(uint256) 1 => -2
//@[gas] run-call: leftShift(uint256) 1 => -2
//@[size] run-call: leftShift(uint256) 1 => -2
//@[none] run-call: leftShift(uint256) 255 => -0x8000000000000000000000000000000000000000000000000000000000000000
//@[gas] run-call: leftShift(uint256) 255 => -0x8000000000000000000000000000000000000000000000000000000000000000
//@[size] run-call: leftShift(uint256) 255 => -0x8000000000000000000000000000000000000000000000000000000000000000
//@[none] run-call: leftShift(uint256) 256 => 0
//@[gas] run-call: leftShift(uint256) 256 => 0
//@[size] run-call: leftShift(uint256) 256 => 0
//@[none] run-call: lessThan() => true
//@[gas] run-call: lessThan() => true
//@[size] run-call: lessThan() => true
//@[none] run-call: greaterThan() => false
//@[gas] run-call: greaterThan() => false
//@[size] run-call: greaterThan() => false
//@[none] run-call: literalLessThan(int256) -10 => true
//@[gas] run-call: literalLessThan(int256) -10 => true
//@[size] run-call: literalLessThan(int256) -10 => true
//@[none] run-call: literalLessThan(int256) -20 => false
//@[gas] run-call: literalLessThan(int256) -20 => false
//@[size] run-call: literalLessThan(int256) -20 => false
//@[none] run-call: parameterLessThan(int256) -20 => true
//@[gas] run-call: parameterLessThan(int256) -20 => true
//@[size] run-call: parameterLessThan(int256) -20 => true
//@[none] run-call: parameterLessThan(int256) -10 => false
//@[gas] run-call: parameterLessThan(int256) -10 => false
//@[size] run-call: parameterLessThan(int256) -10 => false
//@[none] run-call: positiveRemainder() => 5
//@[gas] run-call: positiveRemainder() => 5
//@[size] run-call: positiveRemainder() => 5
//@[none] run-call: largePositiveDivision() => 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[gas] run-call: largePositiveDivision() => 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[size] run-call: largePositiveDivision() => 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@[none] run-call: largePositiveRightShift(uint256) 1 => 0x4000000000000000000000000000000000000000000000000000000000000000
//@[gas] run-call: largePositiveRightShift(uint256) 1 => 0x4000000000000000000000000000000000000000000000000000000000000000
//@[size] run-call: largePositiveRightShift(uint256) 1 => 0x4000000000000000000000000000000000000000000000000000000000000000
//@[none] run-call: largePositiveGreaterThan() => true
//@[gas] run-call: largePositiveGreaterThan() => true
//@[size] run-call: largePositiveGreaterThan() => true

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
