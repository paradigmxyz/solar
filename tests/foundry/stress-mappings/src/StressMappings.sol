// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Stress test for complex nested mappings
/// @notice Tests compiler handling of deeply nested and complex mapping patterns

contract StressMappings {
    // ========== Single-level mappings with various key types ==========
    mapping(uint256 => uint256) public uintToUint;
    mapping(address => uint256) public addressToUint;
    mapping(bytes32 => uint256) public bytes32ToUint;
    mapping(uint256 => address) public uintToAddress;
    mapping(address => address) public addressToAddress;
    mapping(uint256 => bool) public uintToBool;
    mapping(address => bool) public addressToBool;
    mapping(bytes32 => bytes32) public bytes32ToBytes32;
    
    // ========== Two-level nested mappings ==========
    mapping(address => mapping(address => uint256)) public allowances;
    mapping(uint256 => mapping(uint256 => uint256)) public matrix;
    mapping(address => mapping(uint256 => bool)) public addressUintToBool;
    mapping(bytes32 => mapping(address => uint256)) public roleBalances;
    mapping(uint256 => mapping(address => address)) public uintAddressToAddress;
    
    // ========== Three-level nested mappings ==========
    mapping(address => mapping(address => mapping(uint256 => bool))) public permissions;
    mapping(uint256 => mapping(uint256 => mapping(uint256 => uint256))) public cube;
    mapping(address => mapping(uint256 => mapping(bytes32 => uint256))) public complexThree;
    
    // ========== Four-level nested mappings ==========
    mapping(address => mapping(address => mapping(uint256 => mapping(uint256 => uint256)))) public fourLevel;
    mapping(uint256 => mapping(uint256 => mapping(uint256 => mapping(uint256 => bool)))) public hyperCube;
    
    // ========== Five-level nested mapping (extreme case) ==========
    mapping(address => mapping(uint256 => mapping(uint256 => mapping(address => mapping(bytes32 => uint256))))) public fiveLevel;
    
    // ========== Mapping to structs ==========
    struct UserData {
        uint256 balance;
        uint256 lastUpdate;
        bool active;
    }
    
    struct Position {
        int256 x;
        int256 y;
        uint256 timestamp;
    }
    
    mapping(address => UserData) public users;
    mapping(uint256 => Position) public positions;
    mapping(address => mapping(uint256 => UserData)) public userTokenData;
    
    // ========== Single-level setters ==========
    
    function setUintToUint(uint256 key, uint256 value) public {
        uintToUint[key] = value;
    }
    
    function setAddressToUint(address key, uint256 value) public {
        addressToUint[key] = value;
    }
    
    function setBytes32ToUint(bytes32 key, uint256 value) public {
        bytes32ToUint[key] = value;
    }
    
    function setUintToAddress(uint256 key, address value) public {
        uintToAddress[key] = value;
    }
    
    function setAddressToAddress(address key, address value) public {
        addressToAddress[key] = value;
    }
    
    function setUintToBool(uint256 key, bool value) public {
        uintToBool[key] = value;
    }
    
    function setAddressToBool(address key, bool value) public {
        addressToBool[key] = value;
    }
    
    function setBytes32ToBytes32(bytes32 key, bytes32 value) public {
        bytes32ToBytes32[key] = value;
    }
    
    // ========== Single-level getters ==========
    
    function getUintToUint(uint256 key) public view returns (uint256) {
        return uintToUint[key];
    }
    
    function getAddressToUint(address key) public view returns (uint256) {
        return addressToUint[key];
    }
    
    function getBytes32ToUint(bytes32 key) public view returns (uint256) {
        return bytes32ToUint[key];
    }
    
    // ========== Two-level setters ==========
    
    function setAllowance(address owner, address spender, uint256 amount) public {
        allowances[owner][spender] = amount;
    }
    
    function setMatrix(uint256 row, uint256 col, uint256 value) public {
        matrix[row][col] = value;
    }
    
    function setAddressUintToBool(address addr, uint256 id, bool value) public {
        addressUintToBool[addr][id] = value;
    }
    
    function setRoleBalance(bytes32 role, address account, uint256 balance) public {
        roleBalances[role][account] = balance;
    }
    
    // ========== Two-level getters ==========
    
    function getAllowance(address owner, address spender) public view returns (uint256) {
        return allowances[owner][spender];
    }
    
    function getMatrix(uint256 row, uint256 col) public view returns (uint256) {
        return matrix[row][col];
    }
    
    function getAddressUintToBool(address addr, uint256 id) public view returns (bool) {
        return addressUintToBool[addr][id];
    }
    
    function getRoleBalance(bytes32 role, address account) public view returns (uint256) {
        return roleBalances[role][account];
    }
    
    // ========== Three-level setters ==========
    
    function setPermission(address user, address resource, uint256 action, bool allowed) public {
        permissions[user][resource][action] = allowed;
    }
    
    function setCube(uint256 x, uint256 y, uint256 z, uint256 value) public {
        cube[x][y][z] = value;
    }
    
    function setComplexThree(address addr, uint256 id, bytes32 key, uint256 value) public {
        complexThree[addr][id][key] = value;
    }
    
    // ========== Three-level getters ==========
    
    function hasPermission(address user, address resource, uint256 action) public view returns (bool) {
        return permissions[user][resource][action];
    }
    
    function getCube(uint256 x, uint256 y, uint256 z) public view returns (uint256) {
        return cube[x][y][z];
    }
    
    function getComplexThree(address addr, uint256 id, bytes32 key) public view returns (uint256) {
        return complexThree[addr][id][key];
    }
    
    // ========== Four-level setters ==========
    
    function setFourLevel(address a1, address a2, uint256 u1, uint256 u2, uint256 value) public {
        fourLevel[a1][a2][u1][u2] = value;
    }
    
    function setHyperCube(uint256 w, uint256 x, uint256 y, uint256 z, bool value) public {
        hyperCube[w][x][y][z] = value;
    }
    
    // ========== Four-level getters ==========
    
    function getFourLevel(address a1, address a2, uint256 u1, uint256 u2) public view returns (uint256) {
        return fourLevel[a1][a2][u1][u2];
    }
    
    function getHyperCube(uint256 w, uint256 x, uint256 y, uint256 z) public view returns (bool) {
        return hyperCube[w][x][y][z];
    }
    
    // ========== Five-level setters ==========
    
    function setFiveLevel(address a1, uint256 u1, uint256 u2, address a2, bytes32 key, uint256 value) public {
        fiveLevel[a1][u1][u2][a2][key] = value;
    }
    
    // ========== Five-level getters ==========
    
    function getFiveLevel(address a1, uint256 u1, uint256 u2, address a2, bytes32 key) public view returns (uint256) {
        return fiveLevel[a1][u1][u2][a2][key];
    }
    
    // ========== Struct mapping setters ==========
    
    function setUser(address addr, uint256 balance, uint256 lastUpdate, bool active) public {
        users[addr] = UserData(balance, lastUpdate, active);
    }
    
    function setUserBalance(address addr, uint256 balance) public {
        users[addr].balance = balance;
    }
    
    function setUserActive(address addr, bool active) public {
        users[addr].active = active;
    }
    
    function setPosition(uint256 id, int256 x, int256 y) public {
        positions[id] = Position(x, y, block.timestamp);
    }
    
    function setUserTokenData(address addr, uint256 tokenId, uint256 balance, bool active) public {
        userTokenData[addr][tokenId] = UserData(balance, block.timestamp, active);
    }
    
    // ========== Struct mapping getters ==========
    
    function getUser(address addr) public view returns (uint256, uint256, bool) {
        UserData memory u = users[addr];
        return (u.balance, u.lastUpdate, u.active);
    }
    
    function getUserBalance(address addr) public view returns (uint256) {
        return users[addr].balance;
    }
    
    function isUserActive(address addr) public view returns (bool) {
        return users[addr].active;
    }
    
    function getPosition(uint256 id) public view returns (int256, int256, uint256) {
        Position memory p = positions[id];
        return (p.x, p.y, p.timestamp);
    }
    
    function getUserTokenData(address addr, uint256 tokenId) public view returns (uint256, uint256, bool) {
        UserData memory u = userTokenData[addr][tokenId];
        return (u.balance, u.lastUpdate, u.active);
    }
    
    // ========== Complex operations ==========
    
    function incrementBalance(address addr) public {
        users[addr].balance++;
    }
    
    function addToAllowance(address owner, address spender, uint256 amount) public {
        allowances[owner][spender] += amount;
    }
    
    function transferAllowance(address from, address to, address spender, uint256 amount) public {
        require(allowances[from][spender] >= amount, "Insufficient allowance");
        allowances[from][spender] -= amount;
        allowances[to][spender] += amount;
    }
    
    function swapMatrixValues(uint256 r1, uint256 c1, uint256 r2, uint256 c2) public {
        uint256 temp = matrix[r1][c1];
        matrix[r1][c1] = matrix[r2][c2];
        matrix[r2][c2] = temp;
    }
    
    function bulkSetMatrix(uint256[] memory rows, uint256[] memory cols, uint256[] memory values) public {
        require(rows.length == cols.length && cols.length == values.length, "Length mismatch");
        for (uint256 i = 0; i < rows.length; i++) {
            matrix[rows[i]][cols[i]] = values[i];
        }
    }
    
    function sumMatrixRow(uint256 row, uint256 colCount) public view returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < colCount; i++) {
            sum += matrix[row][i];
        }
        return sum;
    }
    
    function countActiveUsers(address[] memory addrs) public view returns (uint256) {
        uint256 count = 0;
        for (uint256 i = 0; i < addrs.length; i++) {
            if (users[addrs[i]].active) {
                count++;
            }
        }
        return count;
    }
}
