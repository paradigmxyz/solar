// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

contract RuntimeERC20 {
    mapping(address => uint256) public balanceOf;

    constructor(address initialHolder, uint256 initialBalance) {
        balanceOf[initialHolder] = initialBalance;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        return true;
    }
}

contract RuntimeNFT {
    mapping(uint256 => address) public ownerOf;
    mapping(address => mapping(address => bool)) public isApprovedForAll;

    constructor() {
        ownerOf[1] = msg.sender;
    }

    function setApprovalForAll(address spender, bool allowed) external {
        isApprovedForAll[msg.sender][spender] = allowed;
    }

    function transferFrom(address from, address to, uint256 tokenId) external {
        require(ownerOf[tokenId] == from, "WRONG_FROM");
        require(msg.sender == from || isApprovedForAll[from][msg.sender], "NOT_AUTHORIZED");
        ownerOf[tokenId] = to;
    }
}

/// @dev Local Zones gas fixture; token execution is identical for every compiler.
contract RuntimeZoneToken {
    string public constant name = "Benchmark USD";
    string public constant symbol = "BENCH";
    string public constant currency = "USD";
    uint64 public constant transferPolicyId = 0;

    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    function setBalance(address account, uint256 amount) external {
        balanceOf[account] = amount;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        if (allowance[from][msg.sender] != type(uint256).max) {
            allowance[from][msg.sender] -= amount;
        }
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
        return true;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        return true;
    }
}

/// @dev The local registry allows the fixture token and refund recipient.
contract RuntimeZoneRegistry {
    function tokenTransferPolicyId(address) external pure returns (bool, uint64) {
        return (true, 0);
    }

    function isAuthorizedRecipient(uint64, address) external pure returns (bool) {
        return true;
    }
}
