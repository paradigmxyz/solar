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
        assert(val == 42);
    }

    function test_IndependentSlots() public {
        m.set(1, 2, 3, 100);
        m.set(1, 2, 4, 200);
        m.set(1, 3, 3, 300);
        m.set(2, 2, 3, 400);

        assert(m.get(1, 2, 3) == 100);
        assert(m.get(1, 2, 4) == 200);
        assert(m.get(1, 3, 3) == 300);
        assert(m.get(2, 2, 3) == 400);
    }

    function test_DefaultZero() public view {
        assert(m.get(99, 99, 99) == 0);
    }
}
