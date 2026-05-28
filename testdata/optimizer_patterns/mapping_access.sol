// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for mapping access optimizations
/// @dev Solar should use scratch space and optimize slot computation

contract MappingAccess {
    // Simple mappings
    mapping(address => uint256) public balances;
    mapping(address => mapping(address => uint256)) public allowances;
    mapping(bytes32 => bool) public executed;
    mapping(uint256 => address) public owners;

    /// @dev Single mapping access
    /// Optimal: Use scratch space, mstore(0x0c, seed), mstore(0x00, key)
    function getBalance(address user) public view returns (uint256) {
        return balances[user];
    }

    /// @dev Single mapping write
    function setBalance(address user, uint256 amount) public {
        balances[user] = amount;
    }

    /// @dev Multiple accesses to same mapping - should reuse slot seed
    function transfer(address from, address to, uint256 amount) public {
        require(balances[from] >= amount, "Insufficient balance");
        unchecked {
            balances[from] -= amount;
            balances[to] += amount;
        }
    }

    /// @dev Nested mapping access
    /// Two keccak256 computations needed
    function getAllowance(address owner, address spender) public view returns (uint256) {
        return allowances[owner][spender];
    }

    /// @dev Nested mapping write
    function approve(address spender, uint256 amount) public {
        allowances[msg.sender][spender] = amount;
    }

    /// @dev Read-modify-write pattern
    function increaseBalance(address user, uint256 amount) public {
        balances[user] += amount;
    }

    /// @dev Multiple mappings in one function
    function transferFrom(address from, address to, uint256 amount) public {
        uint256 allowed = allowances[from][msg.sender];
        require(allowed >= amount, "Insufficient allowance");
        require(balances[from] >= amount, "Insufficient balance");
        
        unchecked {
            allowances[from][msg.sender] = allowed - amount;
            balances[from] -= amount;
            balances[to] += amount;
        }
    }

    /// @dev Bytes32 key mapping
    function markExecuted(bytes32 txHash) public {
        require(!executed[txHash], "Already executed");
        executed[txHash] = true;
    }

    /// @dev Integer key mapping
    function setOwner(uint256 tokenId, address owner) public {
        owners[tokenId] = owner;
    }

    /// @dev Check and update pattern - common in ERC20
    function safeTransfer(address to, uint256 amount) public returns (bool) {
        uint256 senderBalance = balances[msg.sender];
        if (senderBalance < amount) {
            return false;
        }
        unchecked {
            balances[msg.sender] = senderBalance - amount;
            balances[to] += amount;
        }
        return true;
    }
}
