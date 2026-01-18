// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test case for SSTORE dead store elimination
/// @dev Write-after-write to same slot should keep only final write
///
/// Gas savings per eliminated SSTORE:
/// - SSTORE: 2900 - 20000 gas depending on value
/// - Minimum savings: 2900 gas per eliminated store

contract WriteAfterWrite {
    uint256 public value;
    uint256 public counter;

    /// @dev Simple write-after-write - only final write should remain
    /// Before: 2 x SSTORE = ~5800+ gas
    /// After:  1 x SSTORE = ~2900+ gas
    function simpleOverwrite(uint256 newValue) external {
        value = 100;        // Dead store - will be overwritten
        value = newValue;   // Only this store matters
    }

    /// @dev Multiple overwrites - only final write matters
    function multipleOverwrites(uint256 a, uint256 b, uint256 c) external {
        value = a;  // Dead store
        value = b;  // Dead store
        value = c;  // Only this matters
    }

    /// @dev Overwrite with computation
    function computedOverwrite(uint256 x) external {
        value = x;              // Dead store
        value = x * 2;          // Dead store
        value = x * 2 + 1;      // Only this matters
    }

    /// @dev Write same value back (no-op store)
    function writeBackSameValue() external {
        uint256 current = value;    // SLOAD
        value = current;            // Store same value - should be eliminated
    }

    /// @dev Read-modify-write pattern (not a dead store)
    function readModifyWrite(uint256 increment) external {
        uint256 current = value;    // SLOAD
        value = current + increment; // This store is needed
    }

    /// @dev Different slots - no elimination
    uint256 public slotA;
    uint256 public slotB;

    function writeDifferentSlots(uint256 a, uint256 b) external {
        slotA = a;  // Different slot
        slotB = b;  // Different slot - no elimination
    }

    /// @dev Mixed pattern
    function mixedPattern(uint256 x, uint256 y) external {
        slotA = x;      // First write to slotA
        slotB = y;      // Write to slotB
        slotA = x + 1;  // Overwrites first write to slotA (dead store elimination)
    }
}
