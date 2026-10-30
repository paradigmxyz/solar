//@ codegen-matrix: standard
//@ run-call: signedMax -1 => 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: lowOdd 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x7fffffffffffffffffffffffffffffff
//@ run-call: highAddress 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffff0000000000000000000000000000000000000000
//@ run-call: highAddress 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0 => 0x123456789abcdef0123456780000000000000000000000000000000000000000
//@ run-call: highOdd 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffff80000000000000
//@ run-call: signedMax -57896044618658097711785492504343953926634992332820282019728792003956564819968 => 0
//@ run-call: highTop 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: clearAddress 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffff0000000000000000000000000000000000000000

contract ShiftedMasks {
    function signedMax(int256 x) external pure returns (int256) {
        return x & type(int256).max;
    }

    function lowOdd(uint256 x) external pure returns (uint256) {
        return x & (type(uint256).max >> 129);
    }

    function highAddress(uint256 x) external pure returns (uint256) {
        return x & (type(uint256).max << 160);
    }

    function highOdd(uint256 x) external pure returns (uint256) {
        return x & (type(uint256).max << 55);
    }

    function highTop(uint256 x) external pure returns (uint256) {
        return x & (type(uint256).max << 255);
    }

    function clearAddress(uint256 x) external pure returns (uint256) {
        return x & ~uint256(type(uint160).max);
    }
}
