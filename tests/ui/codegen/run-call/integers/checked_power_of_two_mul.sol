//@ codegen-matrix: standard
//@ run-call: double 0x0 => 0x0
//@ run-call: double 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe
//@ run-call-fail: double 0x8000000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: double 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scale32 0x0 => 0x0
//@ run-call: scale32 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
//@ run-call-fail: scale32 0x800000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: scale32 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: shiftHigh 0x0 => 0x0
//@ run-call: shiftHigh 0xffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffff00000000000000000000000000000000
//@ run-call-fail: shiftHigh 0x100000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: shiftHigh 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: top 0x0 => 0x0
//@ run-call: top 0x1 => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: top 0x2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: top 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: lengthBytes 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0
//@ run-call-fail: lengthBytes 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scaleTwice 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe1
//@ run-call-fail: scaleTwice 0x800000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: triple 0x5555555555555555555555555555555555555555555555555555555555555555 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: triple 0x5555555555555555555555555555555555555555555555555555555555555556 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// A checked product by a power of two tests the factor's high bits; other constants keep the
// quotient test.
contract CheckedPowerOfTwoMul {
    function double(uint256 x) external pure returns (uint256) {
        return x * 2;
    }

    function scale32(uint256 x) external pure returns (uint256) {
        return x * 32;
    }

    function shiftHigh(uint256 x) external pure returns (uint256) {
        return x * 2 ** 128;
    }

    function top(uint256 x) external pure returns (uint256) {
        return x * 2 ** 255;
    }

    function lengthBytes(uint256 len) external pure returns (uint256) {
        return (len + 1) * 32;
    }

    function scaleTwice(uint256 x) external pure returns (uint256 p, uint256 q) {
        p = x * 32;
        q = p + 1;
    }

    function triple(uint256 x) external pure returns (uint256) {
        return x * 3;
    }
}
