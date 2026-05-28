// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test patterns for event emission optimizations
/// @dev Solar should use scratch space and pre-compute signatures

contract EventEmit {
    // Events with various signatures
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event Deposit(address indexed user, uint256 amount);
    event Withdrawal(address indexed user, uint256 amount);
    
    // Event with no indexed params - all data
    event Log(string message, uint256 value);
    
    // Event with only indexed params - no data
    event Ping(address indexed sender);
    
    // Anonymous event
    event AnonymousLog(uint256 indexed topic1, uint256 data) anonymous;

    address public owner;
    mapping(address => uint256) public balances;

    constructor() {
        owner = msg.sender;
        emit OwnershipTransferred(address(0), msg.sender);
    }

    /// @dev Standard ERC20-style transfer event
    /// Optimal: mstore(0x00, amount); log3(0x00, 0x20, sig, from, to)
    function emitTransfer(address from, address to, uint256 amount) public {
        emit Transfer(from, to, amount);
    }

    /// @dev Using msg.sender in event
    function deposit() public payable {
        balances[msg.sender] += msg.value;
        emit Deposit(msg.sender, msg.value);
    }

    /// @dev Multiple events in one function
    function transferOwnership(address newOwner) public {
        require(msg.sender == owner, "Not owner");
        require(newOwner != address(0), "Zero address");
        
        address oldOwner = owner;
        owner = newOwner;
        
        emit OwnershipTransferred(oldOwner, newOwner);
    }

    /// @dev Event with computed values
    function withdraw(uint256 amount) public {
        require(balances[msg.sender] >= amount, "Insufficient balance");
        
        unchecked {
            balances[msg.sender] -= amount;
        }
        
        payable(msg.sender).transfer(amount);
        emit Withdrawal(msg.sender, amount);
    }

    /// @dev Event with string data - requires memory allocation
    function log(string memory message, uint256 value) public {
        emit Log(message, value);
    }

    /// @dev Event with no data (only topics)
    /// Optimal: log2(0, 0, sig, sender)
    function ping() public {
        emit Ping(msg.sender);
    }

    /// @dev Anonymous event - saves gas by not emitting signature
    function anonymousLog(uint256 topic, uint256 data) public {
        emit AnonymousLog(topic, data);
    }

    /// @dev Event inside loop - scratch space reuse opportunity
    function batchTransfer(address[] calldata recipients, uint256 amount) public {
        uint256 total = recipients.length * amount;
        require(balances[msg.sender] >= total, "Insufficient balance");
        
        unchecked {
            balances[msg.sender] -= total;
            for (uint256 i = 0; i < recipients.length; i++) {
                balances[recipients[i]] += amount;
                emit Transfer(msg.sender, recipients[i], amount);
            }
        }
    }
}
