// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

contract Owned {
    address immutable owner;
    uint256 immutable fee;

    constructor(address o, uint256 f) {
        owner = o;
        fee = f;
    }

    function quote(uint256 amount) external view returns (uint256) {
        if (msg.sender == owner) return amount;
        return amount + fee;
    }
}
