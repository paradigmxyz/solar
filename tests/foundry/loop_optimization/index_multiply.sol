// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Index Multiply Test
/// @notice Test case for strength reduction - i * 32 pattern
contract IndexMultiply {
    /// @notice Computes offsets for array access
    /// @dev Pattern: i * 32 can be replaced with accumulator += 32
    function computeOffsets(uint256 count) public pure returns (uint256[] memory offsets) {
        offsets = new uint256[](count);
        for (uint256 i = 0; i < count; i++) {
            // Before strength reduction: multiply each iteration (MUL = 5 gas)
            // After: accumulator += 32 each iteration (ADD = 3 gas)
            offsets[i] = i * 32;
        }
    }

    /// @notice Memory slot computation
    /// @dev Another i * constant pattern
    function computeSlots(uint256 count) public pure returns (uint256[] memory slots) {
        slots = new uint256[](count);
        for (uint256 i = 0; i < count; i++) {
            // i * 0x20 is common for memory word access
            slots[i] = i * 0x20;
        }
    }

    /// @notice Multiple strength reduction opportunities
    function multiplePatterns(uint256 n) public pure returns (uint256 sum1, uint256 sum2) {
        for (uint256 i = 0; i < n; i++) {
            // Two different multiplication patterns
            sum1 += i * 5;   // Can become acc1 += 5
            sum2 += i * 10;  // Can become acc2 += 10
        }
    }

    /// @notice Already optimized version for gas comparison
    function computeOffsetsOptimized(uint256 count) public pure returns (uint256[] memory offsets) {
        offsets = new uint256[](count);
        uint256 offset = 0;
        for (uint256 i = 0; i < count; i++) {
            offsets[i] = offset;
            offset += 32;
        }
    }
}
