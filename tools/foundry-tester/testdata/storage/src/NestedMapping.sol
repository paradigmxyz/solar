// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Nested Mapping Tests
/// @notice Tests for mapping(key => mapping(key => value)) patterns

contract NestedMapping {
    // Two-level nested mapping (common pattern: allowances, balances per token)
    mapping(address => mapping(address => uint256)) public allowances;
    
    // Three-level nested mapping
    mapping(address => mapping(address => mapping(uint256 => bool))) public permissions;
    
    // Mapping with uint keys
    mapping(uint256 => mapping(uint256 => uint256)) public matrix;
    
    // Set allowance (like ERC20 approve)
    function setAllowance(address owner, address spender, uint256 amount) public {
        allowances[owner][spender] = amount;
    }
    
    // Get allowance
    function getAllowance(address owner, address spender) public view returns (uint256) {
        return allowances[owner][spender];
    }
    
    // Increase allowance
    function increaseAllowance(address owner, address spender, uint256 addedValue) public {
        allowances[owner][spender] += addedValue;
    }
    
    // Set matrix value
    function setMatrix(uint256 row, uint256 col, uint256 value) public {
        matrix[row][col] = value;
    }
    
    // Get matrix value  
    function getMatrix(uint256 row, uint256 col) public view returns (uint256) {
        return matrix[row][col];
    }
    
    // Set permission (three levels deep)
    function setPermission(address user, address resource, uint256 action, bool allowed) public {
        permissions[user][resource][action] = allowed;
    }
    
    // Get permission
    function hasPermission(address user, address resource, uint256 action) public view returns (bool) {
        return permissions[user][resource][action];
    }
}
