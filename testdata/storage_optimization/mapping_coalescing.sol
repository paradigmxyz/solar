// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test case for mapping slot coalescing
/// @dev Mappings with same key should use cached slot computation and value
///
/// Mapping slot computation: keccak256(key, slot)
/// - Same key + same mapping base = same storage slot
/// - Multiple accesses should be coalesced

contract MappingCoalescing {
    mapping(address => uint256) public balances;
    mapping(address => uint256) public nonces;
    mapping(address => mapping(address => uint256)) public allowances;

    /// @dev Multiple reads from same mapping key
    function multipleReads(address user) external view returns (uint256) {
        uint256 a = balances[user];  // SLOAD (compute slot once)
        uint256 b = balances[user];  // Should reuse value
        uint256 c = balances[user];  // Should reuse value
        return a + b + c;
    }

    /// @dev Read-modify-write to mapping
    function readModifyWrite(address user, uint256 amount) external {
        uint256 current = balances[user];  // SLOAD
        balances[user] = current + amount;  // SSTORE
    }

    /// @dev ERC20-style transfer - multiple mapping accesses
    function transfer(address from, address to, uint256 amount) external {
        uint256 fromBalance = balances[from];  // SLOAD balances[from]
        require(fromBalance >= amount, "Insufficient");

        // Multiple accesses to balances[from] should reuse slot
        balances[from] = fromBalance - amount;  // SSTORE

        // Different key - separate slot
        uint256 toBalance = balances[to];  // SLOAD balances[to]
        balances[to] = toBalance + amount;  // SSTORE
    }

    /// @dev Nested mapping access
    function checkAndUpdateAllowance(
        address owner,
        address spender,
        uint256 amount
    ) external {
        uint256 allowed = allowances[owner][spender];  // SLOAD
        require(allowed >= amount, "Insufficient allowance");

        // Same nested key should reuse
        uint256 remaining = allowances[owner][spender];  // Should reuse value
        allowances[owner][spender] = remaining - amount;  // SSTORE
    }

    /// @dev Multiple different mappings, same key
    function accessMultipleMappings(address user) external view returns (
        uint256 balance,
        uint256 nonce
    ) {
        // Different mappings = different base slots = different final slots
        balance = balances[user];  // SLOAD (slot = keccak(user, balances.slot))
        nonce = nonces[user];      // SLOAD (slot = keccak(user, nonces.slot))
    }

    /// @dev Write-after-write to same mapping slot
    function overwriteMapping(address user) external {
        balances[user] = 100;  // Dead store (same key)
        balances[user] = 200;  // Only this matters
    }

    /// @dev Conditional access pattern
    function conditionalAccess(address user, bool flag) external returns (uint256) {
        uint256 current = balances[user];  // SLOAD

        if (flag) {
            // This read should reuse the cached value
            return balances[user];
        }

        // This should also reuse
        return current + balances[user];
    }

    // Array-like mapping pattern
    mapping(uint256 => address) public owners;
    mapping(uint256 => uint256) public timestamps;

    /// @dev Access by index
    function getOwnerAndTimestamp(uint256 tokenId) external view returns (
        address owner,
        uint256 timestamp
    ) {
        owner = owners[tokenId];        // SLOAD
        timestamp = timestamps[tokenId]; // Different mapping, same index
    }

    /// @dev Update multiple values for same index
    function updateToken(uint256 tokenId, address newOwner, uint256 newTimestamp) external {
        owners[tokenId] = newOwner;         // SSTORE
        timestamps[tokenId] = newTimestamp;  // SSTORE (different slot)
    }
}
