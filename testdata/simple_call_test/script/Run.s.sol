// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {SimpleTest} from "../test/Test.sol";

contract RunScript {
    function run() external {
        SimpleTest t = new SimpleTest();
        bool result = t.runTest();
        require(result, "Test failed");
    }
}
