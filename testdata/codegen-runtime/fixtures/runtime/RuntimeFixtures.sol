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
