// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test case for SLOAD coalescing optimization
/// @dev Multiple reads from same slot should use single SLOAD
///
/// Gas savings per coalesced SLOAD:
/// - Cold SLOAD: 2100 gas
/// - Warm SLOAD: 100 gas  
/// - Savings: 2000 gas per eliminated redundant load

contract MultipleReads {
    uint256 public value;
    uint256 public result;

    /// @dev Three reads from same slot - should become 1 SLOAD
    /// Before: 3 x SLOAD = 2100 + 100 + 100 = 2300 gas
    /// After:  1 x SLOAD = 2100 gas (saves 200 gas)
    function readThreeTimes() external view returns (uint256) {
        uint256 a = value;      // First SLOAD (cold)
        uint256 b = value;      // Should reuse cached value
        uint256 c = value;      // Should reuse cached value
        return a + b + c;
    }

    /// @dev Read-modify-write pattern with multiple reads
    function readModifyWrite() external {
        uint256 current = value;    // First SLOAD
        uint256 doubled = value * 2; // Should reuse cached value
        result = current + doubled;
    }

    /// @dev Loop pattern - each iteration reads same slot
    /// The optimizer should hoist the SLOAD out of the loop
    function sumInLoop(uint256 iterations) external view returns (uint256 sum) {
        for (uint256 i = 0; i < iterations; i++) {
            sum += value;  // Should load once before loop
        }
    }

    /// @dev Conditional reads from same slot
    function conditionalReads(bool flag) external view returns (uint256) {
        uint256 a = value;  // First SLOAD
        if (flag) {
            return value;   // Should reuse cached value
        }
        return a + value;   // Should reuse cached value
    }

    /// @dev Multiple slots - each slot should be cached separately
    uint256 public slot1;
    uint256 public slot2;

    function readMultipleSlots() external view returns (uint256) {
        uint256 a = slot1;  // SLOAD slot1
        uint256 b = slot2;  // SLOAD slot2
        uint256 c = slot1;  // Should reuse slot1 cache
        uint256 d = slot2;  // Should reuse slot2 cache
        return a + b + c + d;
    }
}
