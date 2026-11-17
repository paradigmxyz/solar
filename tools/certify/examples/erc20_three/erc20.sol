// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// An ERC20 token's transfer, approval and allowance-spending functions.
contract Erc20 {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;
    uint256 public totalSupply;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    /// @custom:succeeds-if msg.value == 0 && (balanceOf[msg.sender] >= amount && (msg.sender == to || balanceOf[to] + amount < 2 ** 256))
    /// @custom:ensures msg.sender != to ==> balanceOf[msg.sender] == old(balanceOf[msg.sender]) - amount
    /// @custom:ensures msg.sender != to ==> balanceOf[to] == old(balanceOf[to]) + amount
    /// @custom:ensures msg.sender == to ==> balanceOf[to] == old(balanceOf[to])
    /// @custom:ensures forall address a :: a != msg.sender && a != to ==> balanceOf[a] == old(balanceOf[a])
    /// @custom:ensures totalSupply == old(totalSupply)
    /// @custom:ensures result == true
    function transfer(address to, uint256 amount) external returns (bool) {
        uint256 bal = balanceOf[msg.sender];
        require(bal >= amount, "insufficient balance");
        balanceOf[msg.sender] = bal - amount;
        balanceOf[to] += amount;
        emit Transfer(msg.sender, to, amount);
        return true;
    }

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures allowance[msg.sender][spender] == amount
    /// @custom:ensures totalSupply == old(totalSupply)
    /// @custom:ensures forall address a :: balanceOf[a] == old(balanceOf[a])
    /// @custom:ensures result == true
    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    /// @custom:succeeds-if msg.value == 0 && (allowance[from][msg.sender] >= amount && balanceOf[from] >= amount && (from == to || balanceOf[to] + amount < 2 ** 256))
    /// @custom:ensures allowance[from][msg.sender] == old(allowance[from][msg.sender]) - amount
    /// @custom:ensures from != to ==> balanceOf[from] == old(balanceOf[from]) - amount
    /// @custom:ensures from != to ==> balanceOf[to] == old(balanceOf[to]) + amount
    /// @custom:ensures from == to ==> balanceOf[to] == old(balanceOf[to])
    /// @custom:ensures forall address a :: a != from && a != to ==> balanceOf[a] == old(balanceOf[a])
    /// @custom:ensures totalSupply == old(totalSupply)
    /// @custom:ensures result == true
    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        uint256 allowed = allowance[from][msg.sender];
        require(allowed >= amount, "insufficient allowance");
        allowance[from][msg.sender] = allowed - amount;
        uint256 bal = balanceOf[from];
        require(bal >= amount, "insufficient balance");
        balanceOf[from] = bal - amount;
        balanceOf[to] += amount;
        emit Transfer(from, to, amount);
        return true;
    }
}
