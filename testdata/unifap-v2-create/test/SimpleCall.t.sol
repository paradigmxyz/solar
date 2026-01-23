// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Counter} from "../src/Counter.sol";

contract SimpleCallTest {
    Counter public counter;
    bool public setupDone;

    function setUp() public {
        counter = new Counter();
        counter.setNumber(42);
        setupDone = true;
    }

    function testSetupRan() public view returns (bool) {
        return setupDone;
    }
}
