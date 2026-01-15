// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Counter.sol";

contract CounterTest {
    Counter public counter;

    function setUp() public {
        counter = new Counter();
    }

    function test_InitialCountIsZero() public view {
        require(counter.count() == 0, "initial count should be 0");
    }

    function test_Increment() public {
        counter.increment();
        require(counter.count() == 1, "count should be 1");
    }

    function test_IncrementTwice() public {
        counter.increment();
        counter.increment();
        require(counter.count() == 2, "count should be 2");
    }

    function test_GetCount() public {
        counter.increment();
        require(counter.getCount() == 1, "getCount should be 1");
    }
}
