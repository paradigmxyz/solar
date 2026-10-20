// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Sum Array Test
/// @notice Test case for loop invariant code motion - arr.length hoisting
contract SumArray {
    /// @notice Sums all elements in an array
    /// @dev The arr.length should be hoisted out of the loop
    function sum(uint256[] memory arr) public pure returns (uint256 total) {
        // Before optimization: arr.length is computed every iteration
        // After LICM: arr.length is computed once before the loop
        for (uint256 i = 0; i < arr.length; i++) {
            total += arr[i];
        }
    }

    /// @notice Sums with explicit length caching (baseline for gas comparison)
    function sumOptimized(uint256[] memory arr) public pure returns (uint256 total) {
        uint256 len = arr.length;
        for (uint256 i = 0; i < len; i++) {
            total += arr[i];
        }
    }
}
