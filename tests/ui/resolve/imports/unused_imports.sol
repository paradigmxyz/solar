//@ compile-flags: -Zcheck-unused

import "./auxiliary/Library.sol";
import "./auxiliary/Library2.sol" as Lib2;
//~^ WARN: unused import
import {Helper, Utils as U} from "./auxiliary/Helpers.sol";
//~^ WARN: unused import
//~| WARN: unused import
import * as AllHelpers from "./auxiliary/AllHelpers.sol";
//~^ WARN: unused import
