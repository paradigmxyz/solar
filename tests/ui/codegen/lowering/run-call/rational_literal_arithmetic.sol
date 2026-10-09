//@ codegen-matrix: standard
//@ run-call: thirds => 1
//@ run-call: halves => 5
//@ run-call: negativeExponent => 2
//@ run-call: fractionalBase => 2
//@ run-call: negativeBase => -2
//@ run-call: fractionalRemainder => 3
//@ run-call: negativeRemainder => -3
//@ run-call: wideIntermediate => 1024
//@ run-call: maxIntermediate => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: decimals => 1000000000000000000
//@ run-call: negation => -1
//@ run-call: comparison => true, false, true
//@ run-call: typedDivision => 2, -2
//@ run-call: discarded => 1

// Literal arithmetic is exact: fractions only need an integral final value.
contract RationalLiteralArithmetic {
    int256 constant SEVEN = 7;

    function thirds() external pure returns (uint256) {
        return 1 / 3 * 3;
    }

    function halves() external pure returns (uint256) {
        return 5 / 2 * 2;
    }

    function negativeExponent() external pure returns (uint256) {
        return 2 ** -2 * 8;
    }

    function fractionalBase() external pure returns (uint256) {
        return 0.5 ** 3 * 16;
    }

    function negativeBase() external pure returns (int256) {
        return (-1 / 2) ** -1;
    }

    function fractionalRemainder() external pure returns (uint256) {
        return 7.5 % 2 * 2;
    }

    function negativeRemainder() external pure returns (int256) {
        return -7.5 % 2 * 2;
    }

    function wideIntermediate() external pure returns (uint256) {
        return (1 << 300) >> 290;
    }

    function maxIntermediate() external pure returns (uint256) {
        return (2**256 + 1) * 2 - 2**256 - 3;
    }

    function decimals() external pure returns (uint256) {
        return 10 ** -18 * 1 ether * 1 ether;
    }

    function negation() external pure returns (int256) {
        return -(0.25 * 4);
    }

    function comparison() external pure returns (bool, bool, bool) {
        return (0.3 < 0.5, 0.25 == 0.75, -0.5 <= -1 / 2);
    }

    // Typed constants keep integer division.
    function typedDivision() external pure returns (int256, int256) {
        return (SEVEN / 3, -SEVEN / 3);
    }

    function discarded() external pure returns (uint256) {
        0.5;
        (1 / 3, 2.5);
        return 1;
    }
}
