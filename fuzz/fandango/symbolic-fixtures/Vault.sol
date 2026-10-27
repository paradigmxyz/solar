// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Vault {
    bool public paused;
    uint8 public level;
    uint256 public total;
    mapping(address => uint256) public bal;

    function add(uint256 x) external {
        require(!paused);
        total += x;
        bal[msg.sender] += x;
    }

    function read() external view returns (uint256) {
        return total + level;
    }
}
