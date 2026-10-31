//@ codegen-matrix: standard
//@ run-call: matches 0x0101010101010101010101010101010101010101010101010101010101010101, 0x3030303030303030303030303030303030303030303030303030303030303030 => true
//@ run-call: matches 0, 0 => false
//@ run-call: matches 0x0101010101010101010101010101010101010101010101010101010101010101, 0 => false
//@ run-call: errorSelector => 0x8c379a000000000000000000000000000000000000000000000000000000000
//@ run-call: shiftedNot => 0xffffffffffffffffffffffffffffffffffffffffffffffffaa00000000000000
//@ run-call: invertedShiftRight => 0x3fffffffffffffffffffffffffffffffffffffffffffffffffffffffbfffff0c
//@ run-call: reciprocalTenThousand => 0x68db8bac710cb295e9e1b089a027525460aa64c2f837b4a2339c0ebedfa43
//@ run-call: reciprocalWad => 0x12725dd1d243aba0e75fe645cc4873f9e65afe688c928e1f21
//@ run-call: reciprocalDay => 0xc22e450672894ab6cd8efb11d33f5617839a5bc7dea00c22e450672894ab
//@ run-call: repeatedFactor => 0x1234123412341234123412341234123412341234123412341234123412341234
//@ run-call: orSplit => 0xffffffffffffffffffffffff00000000000000000000000000000000000000ff
//@ run-call: subSplit => 0x1234567800000000ffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: p256Prime => 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffff
//@ run-call: p256Order => 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551
//@ run-call: square => 0x14b66dc33f6acdca878d6495a927ab9717824ef4ebaacbcca4ab582281edee1
//@ run-call: wad => 0xde0b6b3a7640000
//@ run-call: wadSquared => 0xc097ce7bc90715b34b9f1000000000
//@ run-call: scaledPowerOfTen => 0x9bd0d8a976ad77f90fa8000000
//@ run-call: repeatedHalf => 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef

contract RepeatedPatterns {
    function matches(uint256 a, uint256 b) external pure returns (bool) {
        return a == 0x0101010101010101010101010101010101010101010101010101010101010101
            && b == 0x3030303030303030303030303030303030303030303030303030303030303030;
    }

    function errorSelector() external pure returns (uint256) {
        return 0x8c379a000000000000000000000000000000000000000000000000000000000;
    }

    function shiftedNot() external pure returns (uint256) {
        return 0xffffffffffffffffffffffffffffffffffffffffffffffffaa00000000000000;
    }

    function invertedShiftRight() external pure returns (uint256) {
        return 0x3fffffffffffffffffffffffffffffffffffffffffffffffffffffffbfffff0c;
    }

    function reciprocalTenThousand() external pure returns (uint256) {
        return 0x68db8bac710cb295e9e1b089a027525460aa64c2f837b4a2339c0ebedfa43;
    }

    function reciprocalWad() external pure returns (uint256) {
        return 0x12725dd1d243aba0e75fe645cc4873f9e65afe688c928e1f21;
    }

    function reciprocalDay() external pure returns (uint256) {
        return 0xc22e450672894ab6cd8efb11d33f5617839a5bc7dea00c22e450672894ab;
    }

    function repeatedFactor() external pure returns (uint256) {
        return 0x1234123412341234123412341234123412341234123412341234123412341234;
    }

    function orSplit() external pure returns (uint256) {
        return 0xffffffffffffffffffffffff00000000000000000000000000000000000000ff;
    }

    function subSplit() external pure returns (uint256) {
        return 0x1234567800000000ffffffffffffffffffffffffffffffffffffffffffffffff;
    }

    function p256Prime() external pure returns (uint256) {
        return 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffff;
    }

    function p256Order() external pure returns (uint256) {
        return 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551;
    }

    function square() external pure returns (uint256) {
        return 0x14b66dc33f6acdca878d6495a927ab9717824ef4ebaacbcca4ab582281edee1;
    }

    function wad() external pure returns (uint256) {
        return 0xde0b6b3a7640000;
    }

    function wadSquared() external pure returns (uint256) {
        return 0xc097ce7bc90715b34b9f1000000000;
    }

    function scaledPowerOfTen() external pure returns (uint256) {
        return 0x9bd0d8a976ad77f90fa8000000;
    }

    function repeatedHalf() external pure returns (uint256) {
        return 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef;
    }
}
