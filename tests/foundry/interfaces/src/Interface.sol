// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface ICounter {
    function count() external view returns (uint256);
    function increment() external;
}

contract Counter is ICounter {
    uint256 public count;
    
    function increment() external {
        count += 1;
    }
}

contract Caller {
    function callIncrement(address counter) external {
        ICounter(counter).increment();
    }
    
    function getCount(address counter) external view returns (uint256) {
        return ICounter(counter).count();
    }
}
