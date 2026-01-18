// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for storage packing optimizations
/// @dev Solar should optimize storage layout and access patterns

contract PackedStorage {
    // Inefficient: 3 storage slots
    bool public flag1;
    bool public flag2;
    address public owner;
    
    // Efficient: 1 storage slot (Solady pattern)
    // Pack address (160 bits) + flags in high bits
    uint256 private _packedOwnerAndFlags;
    
    uint256 private constant _OWNER_MASK = (1 << 160) - 1;
    uint256 private constant _FLAG1_BIT = 1 << 255;
    uint256 private constant _FLAG2_BIT = 1 << 254;

    /// @dev Store owner with packed flags
    function setOwnerPacked(address newOwner, bool f1, bool f2) public {
        _packedOwnerAndFlags = uint256(uint160(newOwner))
            | (f1 ? _FLAG1_BIT : 0)
            | (f2 ? _FLAG2_BIT : 0);
    }

    /// @dev Get packed owner
    function getOwnerPacked() public view returns (address) {
        return address(uint160(_packedOwnerAndFlags & _OWNER_MASK));
    }

    /// @dev Get flag1 from packed
    function getFlag1Packed() public view returns (bool) {
        return _packedOwnerAndFlags & _FLAG1_BIT != 0;
    }

    /// @dev Set only flag1 (single SLOAD + SSTORE)
    function setFlag1Packed(bool value) public {
        if (value) {
            _packedOwnerAndFlags |= _FLAG1_BIT;
        } else {
            _packedOwnerAndFlags &= ~_FLAG1_BIT;
        }
    }

    // Another common pattern: ERC20-style balance + allowance packing
    struct PackedBalance {
        uint128 balance;
        uint128 allowance;
    }
    
    mapping(address => PackedBalance) public packedBalances;

    /// @dev Update both values in one SSTORE
    function updatePackedBalance(address user, uint128 newBalance, uint128 newAllowance) public {
        packedBalances[user] = PackedBalance(newBalance, newAllowance);
    }

    /// @dev Timestamp packing (uint48 is enough until year 8 million)
    struct TimestampedValue {
        uint208 value;
        uint48 timestamp;
    }
    
    mapping(bytes32 => TimestampedValue) public timestampedValues;

    /// @dev Store value with timestamp
    function storeWithTimestamp(bytes32 key, uint208 value) public {
        timestampedValues[key] = TimestampedValue(value, uint48(block.timestamp));
    }

    // Slot computation optimization pattern from Solady
    // Uses specific slot seeds to minimize keccak input

    uint256 private constant _BALANCE_SLOT_SEED = 0x87a211a2;
    
    /// @dev Optimized mapping slot computation
    function getBalanceSlot(address user) public pure returns (bytes32 slot) {
        assembly {
            mstore(0x0c, _BALANCE_SLOT_SEED)
            mstore(0x00, user)
            slot := keccak256(0x0c, 0x20)
        }
    }

    // Sentinel pattern: use special value to indicate state
    uint256 private constant _NOT_INITIALIZED = 0;
    uint256 private constant _INITIALIZED = 1;
    
    uint256 private _initialized;

    /// @dev Initialize with sentinel check
    function initialize(address newOwner) public {
        require(_initialized == _NOT_INITIALIZED, "Already initialized");
        _initialized = _INITIALIZED;
        owner = newOwner;
    }

    /// @dev Solady-style: pack sentinel with owner
    /// Uses bit 255 as "initialized if owner is zero" sentinel
    function initializePacked(address newOwner) public {
        assembly {
            let slot := _packedOwnerAndFlags.slot
            if sload(slot) {
                mstore(0x00, 0x0dc149f0) // AlreadyInitialized
                revert(0x1c, 0x04)
            }
            // If owner is zero, set bit 255 as sentinel
            sstore(slot, or(shr(96, shl(96, newOwner)), shl(255, iszero(newOwner))))
        }
    }
}
