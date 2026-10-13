//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: negPlusHigh => 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: notPlusHigh => 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: negHigh => -0x8000000000000000000000000000000000000000000000000000000000000000
// Integer literal arithmetic is exact: no intermediate value may overflow or panic.
contract LiteralArithmeticFold {
    function negPlusHigh() external pure returns (uint256) {
        return -1 + (1 << 255);
    }

    function notPlusHigh() external pure returns (uint256) {
        return ~0 + (1 << 255);
    }

    function negHigh() external pure returns (int256) {
        return -(1 << 255);
    }
}
