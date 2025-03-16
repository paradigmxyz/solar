// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "./Counter.sol";

contract AnotherCounter {
    Counter counter = new Counter();

    constructor(Counter _counter) {
        counter = _counter;
    }

    function setNumber(uint256 newNumber) public {
        counter.setNumber(newNumber);
    }

    function increment() public {
        counter.increment();
    }
}
