// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Mapping edge cases
/// @notice Security-focused tests for mappings
contract MappingEdgeCases {
    // Simple mappings
    mapping(uint256 => uint256) public uintMap;
    mapping(address => uint256) public addressMap;
    mapping(bytes32 => uint256) public bytes32Map;

    // Nested mappings
    mapping(address => mapping(address => uint256)) public allowances;
    mapping(uint256 => mapping(uint256 => uint256)) public matrix;

    // ========== Edge Case Keys ==========

    function setZeroKey(uint256 value) public {
        uintMap[0] = value;
    }

    function getZeroKey() public view returns (uint256) {
        return uintMap[0];
    }

    function setMaxKey(uint256 value) public {
        uintMap[type(uint256).max] = value;
    }

    function getMaxKey() public view returns (uint256) {
        return uintMap[type(uint256).max];
    }

    function setAddressZero(uint256 value) public {
        addressMap[address(0)] = value;
    }

    function getAddressZero() public view returns (uint256) {
        return addressMap[address(0)];
    }

    function setBytes32Zero(uint256 value) public {
        bytes32Map[bytes32(0)] = value;
    }

    function getBytes32Zero() public view returns (uint256) {
        return bytes32Map[bytes32(0)];
    }

    // ========== Multiple Keys ==========

    function setMultipleKeys(uint256 k1, uint256 v1, uint256 k2, uint256 v2) public {
        uintMap[k1] = v1;
        uintMap[k2] = v2;
    }

    function getKey(uint256 k) public view returns (uint256) {
        return uintMap[k];
    }

    // ========== Nested Mappings Edge Cases ==========

    function setAllowance(address owner, address spender, uint256 amount) public {
        allowances[owner][spender] = amount;
    }

    function getAllowance(address owner, address spender) public view returns (uint256) {
        return allowances[owner][spender];
    }

    function setAllowanceZeroAddresses(uint256 amount) public {
        allowances[address(0)][address(0)] = amount;
    }

    function getAllowanceZeroAddresses() public view returns (uint256) {
        return allowances[address(0)][address(0)];
    }

    // ========== Matrix Operations ==========

    function setMatrixCell(uint256 row, uint256 col, uint256 value) public {
        matrix[row][col] = value;
    }

    function getMatrixCell(uint256 row, uint256 col) public view returns (uint256) {
        return matrix[row][col];
    }

    function setMatrixCorners(uint256 value) public {
        matrix[0][0] = value;
        matrix[0][type(uint256).max] = value + 1;
        matrix[type(uint256).max][0] = value + 2;
        matrix[type(uint256).max][type(uint256).max] = value + 3;
    }

    function getMatrixCorner(uint256 corner) public view returns (uint256) {
        if (corner == 0) return matrix[0][0];
        if (corner == 1) return matrix[0][type(uint256).max];
        if (corner == 2) return matrix[type(uint256).max][0];
        return matrix[type(uint256).max][type(uint256).max];
    }

    // ========== Overwrite Tests ==========

    function overwriteKey(uint256 k, uint256 v1, uint256 v2) public {
        uintMap[k] = v1;
        uintMap[k] = v2; // Overwrite
    }

    function incrementKey(uint256 k) public {
        uintMap[k] = uintMap[k] + 1;
    }

    // ========== Default Value Tests ==========

    function getUnsetKey(uint256 k) public view returns (uint256) {
        return uintMap[k]; // Should return 0 for unset keys
    }

    function getUnsetNestedKey(address a, address b) public view returns (uint256) {
        return allowances[a][b]; // Should return 0 for unset nested keys
    }
}
