//@ codegen-matrix: standard
//@ run-call: f 100, 255, 6 => 121
//@ run-call: f 0, 0xff, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: f 100, 255, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: f 1, 0xff, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// The bound of a right shift that drops set bits is the shifted bound, not zero.
contract C {
    function f(uint256 s0, uint256 x, uint256 y) external pure returns (uint256) {
        return s0 + (uint8(x) >> 4) + (y & ~uint256(1));
    }
}
