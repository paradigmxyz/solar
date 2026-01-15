// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Payable {
    uint256 public totalReceived;

    function deposit() public payable {
        totalReceived += msg.value;
    }

    function getBalance() public view returns (uint256) {
        return address(this).balance;
    }

    function nonPayable() public pure returns (uint256) {
        return 42;
    }

    receive() external payable {
        totalReceived += msg.value;
    }
}
