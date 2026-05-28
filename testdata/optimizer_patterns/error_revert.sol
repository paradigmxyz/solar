// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for error revert optimizations
/// @dev Solar should emit compact revert code for custom errors

/// Custom errors without parameters - should use minimal revert pattern:
/// mstore(0x00, selector)
/// revert(0x1c, 0x04)
error Unauthorized();
error InsufficientBalance();
error InvalidAmount();
error ZeroAddress();
error AlreadyInitialized();
error TransferFailed();

/// Custom errors with parameters - different optimization
error InsufficientBalanceWithAmount(uint256 required, uint256 available);
error UnauthorizedCaller(address caller);

contract ErrorRevert {
    address public owner;
    mapping(address => uint256) public balances;

    constructor() {
        owner = msg.sender;
    }

    /// @dev Simple revert with no-param error
    /// Optimal: mstore(0x00, 0x82b42900) revert(0x1c, 0x04)
    function onlyOwner() public view {
        if (msg.sender != owner) {
            revert Unauthorized();
        }
    }

    /// @dev Check and revert pattern
    function requireBalance(address user, uint256 amount) public view {
        if (balances[user] < amount) {
            revert InsufficientBalance();
        }
    }

    /// @dev Zero check revert
    function requireNonZero(uint256 amount) public pure {
        if (amount == 0) {
            revert InvalidAmount();
        }
    }

    /// @dev Address zero check
    function requireNonZeroAddress(address addr) public pure {
        if (addr == address(0)) {
            revert ZeroAddress();
        }
    }

    /// @dev Multiple reverts in one function
    function transferWithChecks(address to, uint256 amount) public {
        if (msg.sender == address(0)) {
            revert ZeroAddress();
        }
        if (to == address(0)) {
            revert ZeroAddress();
        }
        if (amount == 0) {
            revert InvalidAmount();
        }
        if (balances[msg.sender] < amount) {
            revert InsufficientBalance();
        }
        
        unchecked {
            balances[msg.sender] -= amount;
            balances[to] += amount;
        }
    }

    /// @dev Revert with parameters - different encoding needed
    function revertWithParams(uint256 required) public view {
        uint256 available = balances[msg.sender];
        if (available < required) {
            revert InsufficientBalanceWithAmount(required, available);
        }
    }

    /// @dev Using require with string (legacy pattern) - less optimal
    function legacyRequire(uint256 amount) public pure {
        require(amount > 0, "Amount must be positive");
    }
}
