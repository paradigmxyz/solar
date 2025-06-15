//@ compile-flags: -Zwarn-unused
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "./auxiliary/Library.sol";
import "./auxiliary/Library2.sol" as Lib2;
import {Helper, Utils as U} from "./auxiliary/Helpers.sol";
import * as AllHelpers from "./auxiliary/AllHelpers.sol";

contract Test {
    // Use all imports
    function test() public {
        Library.libFunc();
        Lib2.lib2Func();
        Helper.doSomething();
        U.utilFunc();
        AllHelpers.AllHelper1.help1();
    }
}