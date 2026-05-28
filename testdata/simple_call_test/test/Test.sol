// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Counter} from "../src/Counter.sol";

contract SimpleTest {
    Counter public counter;
    bool public passed;

    function runTest() external returns (bool) {
        // Deploy Counter
        counter = new Counter();
        
        // Call setNumber(42)
        counter.setNumber(42);
        
        // Verify it worked
        if (counter.number() == 42) {
            passed = true;
            return true;
        }
        return false;
    }
}
