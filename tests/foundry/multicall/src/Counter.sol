// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Counter {
    uint256 public count;
    
    function increment() external returns (uint256) {
        count++;
        return count;
    }
    
    function decrement() external returns (uint256) {
        count--;
        return count;
    }
    
    function setCount(uint256 _count) external {
        count = _count;
    }
    
    function getCount() external view returns (uint256) {
        return count;
    }
    
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }
}
