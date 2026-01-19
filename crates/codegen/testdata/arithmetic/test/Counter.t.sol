// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Counter.sol";

contract CounterTest {
    Counter public counter;

    function setUp() public {
        counter = new Counter();
    }

    function test_InitialCountIsZero() public view {
        assert(counter.count() == 0);
    }

    function test_Increment() public {
        counter.increment();
        assert(counter.count() == 1);
    }

    function test_IncrementTwice() public {
        counter.increment();
        counter.increment();
        assert(counter.count() == 2);
    }

    function test_GetCount() public {
        counter.increment();
        assert(counter.getCount() == 1);
    }
}
