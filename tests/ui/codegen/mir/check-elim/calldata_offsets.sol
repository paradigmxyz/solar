//@ codegen-matrix: standard
//@ run-call: sum [] => 0
//@ run-call: sum [1, 2, 3] => 6
//@ run-call: sum [0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0] => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: sum [0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: pick [5, 6, 7], 2 => 7
//@ run-call-fail: pick [5, 6, 7], 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
pragma solidity ^0.8.0;

// A loop counter's element offset `i * 32` into a calldata array cannot wrap,
// so the loop reads each element without testing the offset.
contract CalldataOffsets {
    function sum(uint256[] calldata values) external pure returns (uint256 total) {
        for (uint256 i; i < values.length; ++i) total += values[i];
    }

    function pick(uint256[] calldata values, uint256 i) external pure returns (uint256) {
        return values[i];
    }
}
