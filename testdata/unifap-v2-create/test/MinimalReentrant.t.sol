// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";
import {ReentrancyGuard} from "solmate/utils/ReentrancyGuard.sol";

contract MinimalReentrant is ReentrancyGuard {
    uint256 public value;
    
    function setValue(uint256 v) external nonReentrant {
        value = v;
    }
    
    function getValue() external view returns (uint256) {
        return value;
    }
}

contract MinimalReentrantTest is Test {
    MinimalReentrant guard;
    
    function setUp() public {
        guard = new MinimalReentrant();
    }
    
    function testSetValue() public {
        guard.setValue(42);
        assertEq(guard.value(), 42);
    }
    
    function testMultipleSets() public {
        guard.setValue(1);
        guard.setValue(2);
        guard.setValue(3);
        assertEq(guard.value(), 3);
    }
}
