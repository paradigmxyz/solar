// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Tests function overload resolution in library inlining
library OverloadLib {
    /// @notice Single-argument find calls the two-argument version
    function find(uint256 key) internal pure returns (uint256) {
        return find(key, true);  // calls different overload
    }
    
    /// @notice Two-argument find is the actual implementation
    function find(uint256 key, bool flag) internal pure returns (uint256) {
        return flag ? key : 0;
    }
    
    /// @notice Chained overload: calls single-arg which calls two-arg
    function findDefault(uint256 key) internal pure returns (uint256) {
        return find(key);
    }
}
