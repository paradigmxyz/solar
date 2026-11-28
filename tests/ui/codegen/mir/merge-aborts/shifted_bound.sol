//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@ run-call: sum 100, 255, 6 => 121
//@ run-call-fail: sum 100, 255, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => Panic(0x11)
// `uint8(x) >> 4` is at most 15, so with the masked word the two addends can reach 2^256 and
// each addition keeps its own overflow test. The second call adds 15 and 2^256 - 2 to 100: the
// sum wraps to 113, which is below 115 but not below 100.
contract ShiftedBound {
    function sum(uint256 base, uint256 x, uint256 y) external pure returns (uint256) {
        return base + (uint8(x) >> 4) + (y & ~uint256(1));
    }
}
