//@ codegen-matrix: standard
//@ run-call: scale 5, 7, 0 => 5
//@ run-call: scale 3, 0, 4 => 0
//@ run-call: scale 3, 2, 10 => 3072
//@ run-call: scale 1, 2, 255 => 57896044618658097711785492504343953926634992332820282019728792003956564819968
//@ run-call-fail: scale 1, 2, 256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scale 0x5555555555555555555555555555555555555555555555555555555555555555, 3, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: scale 0x5555555555555555555555555555555555555555555555555555555555555556, 3, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scale 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: scale 2, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scaleLeft 5, 7, 0 => 5
//@ run-call: scaleLeft 3, 0, 4 => 0
//@ run-call: scaleLeft 3, 2, 10 => 3072
//@ run-call: scaleLeft 1, 2, 255 => 57896044618658097711785492504343953926634992332820282019728792003956564819968
//@ run-call-fail: scaleLeft 1, 2, 256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scaleLeft 0x5555555555555555555555555555555555555555555555555555555555555555, 3, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: scaleLeft 0x5555555555555555555555555555555555555555555555555555555555555556, 3, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: scaleLeft 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: scaleLeft 2, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
pragma solidity ^0.8.0;

// The checked product's factor never changes inside the loop, so the check
// compares `x` with `MAX / y - (y == 0)`: a zero factor never overflows, and
// `MAX / 3` times three is exactly `MAX`, while one more overflows.
contract ProductCheck {
    function scale(uint256 x, uint256 y, uint256 n) external pure returns (uint256) {
        for (uint256 i = 0; i < n; ++i) {
            x = x * y;
        }
        return x;
    }

    function scaleLeft(uint256 x, uint256 y, uint256 n) external pure returns (uint256) {
        for (uint256 i = 0; i < n; ++i) {
            x = y * x;
        }
        return x;
    }
}
