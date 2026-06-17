// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract Payable {
    uint256 public balance;

    function deposit() public payable {
        balance = balance + msg.value;
    }

    function getBalance() public view returns (uint256) {
        return balance;
    }

    function withdraw() public {
        balance = 0;
    }
}
