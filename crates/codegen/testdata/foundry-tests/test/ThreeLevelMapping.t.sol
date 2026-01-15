// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ThreeLevelMapping.sol";

contract ThreeLevelMappingTest {
    ThreeLevelMapping public m;

    function setUp() public {
        m = new ThreeLevelMapping();
    }

    function test_SetAndGet() public {
        m.set(1, 2, 3, 42);
        uint256 val = m.get(1, 2, 3);
        require(val == 42, "should be 42");
    }
    
    function test_IndependentSlots() public {
        m.set(1, 2, 3, 100);
        m.set(1, 2, 4, 200);
        m.set(1, 3, 3, 300);
        m.set(2, 2, 3, 400);
        
        require(m.get(1, 2, 3) == 100, "1-2-3");
        require(m.get(1, 2, 4) == 200, "1-2-4");
        require(m.get(1, 3, 3) == 300, "1-3-3");
        require(m.get(2, 2, 3) == 400, "2-2-3");
    }
    
    function test_DefaultZero() public view {
        require(m.get(99, 99, 99) == 0, "default should be 0");
    }
}
