// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Interface.sol";

contract InterfaceTest {
    Counter public counter;
    Caller public caller;

    function setUp() public {
        counter = new Counter();
        caller = new Caller();
    }

    function test_CounterDirectIncrement() public {
        assert(counter.count() == 0);
        counter.increment();
        assert(counter.count() == 1);
    }

    function test_CounterMultipleIncrements() public {
        counter.increment();
        counter.increment();
        counter.increment();
        assert(counter.count() == 3);
    }

    function test_CallThroughInterface() public {
        assert(caller.getCount(address(counter)) == 0);
        caller.callIncrement(address(counter));
        assert(caller.getCount(address(counter)) == 1);
    }

    function test_MultipleCalls() public {
        caller.callIncrement(address(counter));
        caller.callIncrement(address(counter));
        assert(caller.getCount(address(counter)) == 2);
    }
}
