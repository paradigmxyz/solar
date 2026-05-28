// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Enums {
    enum Status { Pending, Active, Completed, Cancelled }
    enum Size { Small, Medium, Large }
    
    Status public currentStatus;
    Size public currentSize;
    
    function setStatus(Status s) external {
        currentStatus = s;
    }
    
    function getStatus() external view returns (Status) {
        return currentStatus;
    }
    
    function isActive() external view returns (bool) {
        return currentStatus == Status.Active;
    }
    
    function setSize(Size s) external {
        currentSize = s;
    }
    
    function compareStatus(Status a, Status b) external pure returns (bool) {
        return a == b;
    }
    
    function statusToUint(Status s) external pure returns (uint256) {
        return uint256(s);
    }
    
    function uintToStatus(uint256 n) external pure returns (Status) {
        return Status(n);
    }
}
