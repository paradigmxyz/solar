// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Parent {
    uint256 public parentValue;
    
    function setParent(uint256 v) external {
        parentValue = v;
    }
}

contract Child is Parent {
    uint256 public childValue;
    
    function setChild(uint256 v) external {
        childValue = v;
    }
    
    function setBoth(uint256 p, uint256 c) external {
        parentValue = p;
        childValue = c;
    }
}
