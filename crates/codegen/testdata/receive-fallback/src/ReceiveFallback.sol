// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ReceiveFallback {
    uint256 public receiveCalls;
    uint256 public fallbackCalls;
    uint256 public totalReceived;
    
    receive() external payable {
        receiveCalls++;
        totalReceived += msg.value;
    }
    
    fallback() external payable {
        fallbackCalls++;
        totalReceived += msg.value;
    }
    
    function getBalance() external view returns (uint256) {
        return address(this).balance;
    }
}
