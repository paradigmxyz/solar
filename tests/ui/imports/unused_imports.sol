//@ compile-flags: -Zwarn-unused
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "./auxiliary/Library.sol";
import "./auxiliary/Library2.sol" as Lib2;
import {Helper, Utils as U} from "./auxiliary/Helpers.sol";
import * as AllHelpers from "./auxiliary/AllHelpers.sol";

contract Test {
    // Only uses Helper from the named import
    function test() public {
        Helper.doSomething();
    }
}