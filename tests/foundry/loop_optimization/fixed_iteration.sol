// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Fixed Iteration Loop Test
/// @notice Test case for loop unrolling with small fixed trip counts
contract FixedIteration {
    /// @notice Initializes an array with values 0-3
    /// @dev This 4-iteration loop is a candidate for full unrolling
    function initSmall() public pure returns (uint256[4] memory result) {
        for (uint256 i = 0; i < 4; i++) {
            result[i] = i * 10;
        }
    }

    /// @notice Initializes an array with values 0-7
    /// @dev This 8-iteration loop is a candidate for 2x or 4x unrolling
    function initMedium() public pure returns (uint256[8] memory result) {
        for (uint256 i = 0; i < 8; i++) {
            result[i] = i * 10;
        }
    }

    /// @notice Sum of first 4 squares
    /// @dev Can be fully unrolled: 0 + 1 + 4 + 9 = 14
    function sumSquares() public pure returns (uint256 total) {
        for (uint256 i = 0; i < 4; i++) {
            total += i * i;
        }
    }

    /// @notice Double each element (2-iteration inner loop)
    function doubleAll(uint256[4] memory arr) public pure returns (uint256[4] memory result) {
        for (uint256 i = 0; i < 4; i++) {
            uint256 val = arr[i];
            // Inner loop: 2 iterations, perfect for unrolling
            for (uint256 j = 0; j < 2; j++) {
                val = val * 2;
            }
            result[i] = val;
        }
    }
}
