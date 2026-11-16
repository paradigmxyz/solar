// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// A minimal token. The invariant ties the ghost sum of balances to totalSupply, with
/// Certora-style ghost updates written next to every balance write.
/// @custom:ghost uint256 sumBalances
/// @custom:invariant totalSupply == sumBalances
contract Token {
    mapping(address => uint256) public balanceOf;
    uint256 public totalSupply;
    address public owner;

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

    /// @custom:ensures owner == msg.sender
    constructor() {
        owner = msg.sender;
    }

    /// @custom:ensures balanceOf[to] == old(balanceOf[to]) + amount
    /// @custom:ensures totalSupply == old(totalSupply) + amount
    /// @custom:ensures forall address a :: a != to ==> balanceOf[a] == old(balanceOf[a])
    function mint(address to, uint256 amount) external onlyOwner {
        totalSupply += amount;
        /// @custom:ghost sumBalances = sumBalances + amount
        balanceOf[to] += amount;
    }

    /// @custom:ensures msg.sender != to ==> balanceOf[msg.sender] == old(balanceOf[msg.sender]) - amount
    /// @custom:ensures msg.sender != to ==> balanceOf[to] == old(balanceOf[to]) + amount
    /// @custom:ensures forall address a :: a != msg.sender && a != to ==> balanceOf[a] == old(balanceOf[a])
    /// @custom:ensures totalSupply == old(totalSupply)
    function transfer(address to, uint256 amount) external returns (bool) {
        uint256 bal = balanceOf[msg.sender];
        require(bal >= amount, "insufficient");
        balanceOf[msg.sender] = bal - amount;
        balanceOf[to] += amount;
        return true;
    }

    /// @custom:ensures balanceOf[msg.sender] == old(balanceOf[msg.sender]) - amount
    function burn(uint256 amount) external {
        balanceOf[msg.sender] -= amount;
        /// @custom:ghost sumBalances = sumBalances - amount
        totalSupply -= amount;
    }
}
