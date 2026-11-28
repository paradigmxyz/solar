//@ codegen-matrix: standard dump
//@ compile-flags: --optimize-runs 10000
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: decode 0x01020304050607 => 0x01020304, 0x0506
//@ run-call: decode 0xffffffffffff => 0xffffffff, 0xffff
//@ run-call-fail: decode 0x010203 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: decode 0x0102030405 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: sum4 [1, 2, 3, 4] => 10
//@ run-call-fail: sum4 [0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0, 0] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
pragma solidity ^0.8.0;

// Loops that run a literal number of times become that many copies of their body, with no
// test or jump left between them, and each copy keeps its own bounds and overflow checks.
contract CompleteLoops {
    // CHECK-LABEL: fn @u32(
    // CHECK-NOT: phi
    // CHECK: ret
    function u32(bytes calldata data, uint256 start) internal pure returns (uint32 value, uint256 offset) {
        offset = start;
        for (uint256 i = 0; i < 4; i++) {
            value <<= 8;
            value |= uint8(data[offset]);
            offset++;
        }
    }

    function u16(bytes calldata data, uint256 start) internal pure returns (uint16 value, uint256 offset) {
        offset = start;
        for (uint256 i = 0; i < 2; i++) {
            value <<= 8;
            value |= uint8(data[offset]);
            offset++;
        }
    }

    function decode(bytes calldata data) external pure returns (uint32 a, uint16 b) {
        uint256 offset;
        (a, offset) = u32(data, 0);
        (b, offset) = u16(data, offset);
    }

    // CHECK-LABEL: fn @sum4(
    // CHECK-NOT: phi
    // CHECK: returndata
    function sum4(uint256[4] calldata values) external pure returns (uint256 total) {
        for (uint256 i = 0; i < 4; i++) {
            total += values[i];
        }
    }
}
