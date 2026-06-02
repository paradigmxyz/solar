// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

abstract contract Base {
    mapping(address => uint256) public balanceOf;
    uint256 public totalSupply;
    
    function _mint(address to, uint256 amount) internal {
        totalSupply += amount;
        unchecked { balanceOf[to] += amount; }
    }
}

contract Child is Base {
    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}
