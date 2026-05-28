// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test case for storage packing optimization
/// @dev Packed structs should use efficient bitwise access patterns
///
/// Storage packing allows multiple small values in one 32-byte slot:
/// - Reduces SLOAD/SSTORE count
/// - Each SLOAD/SSTORE handles multiple values

contract PackedStruct {
    // Packed struct: all fits in one 32-byte slot
    // uint128 (16 bytes) + uint64 (8 bytes) + uint32 (4 bytes) + uint32 (4 bytes) = 32 bytes
    struct PackedData {
        uint128 balance;
        uint64 timestamp;
        uint32 nonce;
        uint32 flags;
    }

    PackedData public data;

    /// @dev Read all fields - should use single SLOAD with bit extraction
    function readAll() external view returns (
        uint128 balance,
        uint64 timestamp,
        uint32 nonce,
        uint32 flags
    ) {
        PackedData memory d = data;
        return (d.balance, d.timestamp, d.nonce, d.flags);
    }

    /// @dev Write all fields - should use single SSTORE with bit packing
    function writeAll(
        uint128 balance,
        uint64 timestamp,
        uint32 nonce,
        uint32 flags
    ) external {
        data = PackedData(balance, timestamp, nonce, flags);
    }

    /// @dev Read single field - still needs SLOAD but only extracts one field
    function readBalance() external view returns (uint128) {
        return data.balance;
    }

    /// @dev Update single field - read-modify-write with bit masking
    /// Should: SLOAD, mask, OR new value, SSTORE
    function updateNonce(uint32 newNonce) external {
        data.nonce = newNonce;
    }

    /// @dev Update multiple fields atomically
    function updateTimestampAndFlags(uint64 newTimestamp, uint32 newFlags) external {
        data.timestamp = newTimestamp;
        data.flags = newFlags;
    }

    // Multiple packed values in same slot (address + smaller types)
    struct AddressPacked {
        address owner;      // 20 bytes
        uint48 expiry;      // 6 bytes  
        uint32 version;     // 4 bytes
        bool active;        // 1 byte
        bool paused;        // 1 byte (total: 32 bytes)
    }

    AddressPacked public config;

    /// @dev Mixed types packing
    function setConfig(
        address owner,
        uint48 expiry,
        uint32 version,
        bool active,
        bool paused
    ) external {
        config = AddressPacked(owner, expiry, version, active, paused);
    }

    /// @dev Read address and check flags
    function getOwnerIfActive() external view returns (address) {
        if (config.active && !config.paused) {
            return config.owner;
        }
        return address(0);
    }

    // Mapping to packed struct
    mapping(address => PackedData) public userData;

    /// @dev Multiple reads from same packed slot
    function getUserData(address user) external view returns (
        uint128 balance,
        uint32 nonce
    ) {
        // Both reads should use single SLOAD
        balance = userData[user].balance;
        nonce = userData[user].nonce;
    }

    /// @dev Update user data atomically
    function updateUserData(
        address user,
        uint128 newBalance,
        uint64 newTimestamp
    ) external {
        PackedData storage d = userData[user];
        d.balance = newBalance;
        d.timestamp = newTimestamp;
    }
}
