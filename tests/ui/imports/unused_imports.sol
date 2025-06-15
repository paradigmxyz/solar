//@ compile-flags: -Zwarn-unused
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "./auxiliary/Library.sol";
import "./auxiliary/Library2.sol" as Lib2; //~ WARN: unused import
import {Helper, Utils as U} from "./auxiliary/Helpers.sol";
import * as AllHelpers from "./auxiliary/AllHelpers.sol"; //~ WARN: unused import

contract Test {
    // Only uses Helper from the named import
    function test() public {
        Helper.doSomething();
    }
    
    // This private function is unused and should trigger a warning
    function unusedPrivate() private {
        // Never called
    }
}