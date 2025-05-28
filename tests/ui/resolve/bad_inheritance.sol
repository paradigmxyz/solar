import "./bad_inheritance.sol" as self1;
import * as self2 from "./bad_inheritance.sol";
import {does_not_exist} from "./bad_inheritance.sol"; //~ ERROR: not found in

contract C is C {} //~ ERROR: contracts cannot inherit from themselves
contract D is self1.D {} //~ ERROR: contracts cannot inherit from themselves
contract E is self2.E {} //~ ERROR: contracts cannot inherit from themselves

contract F is self1 {} //~ ERROR: expected contract, found namespace
//~^ ERROR: expected base class, found namespace
contract G is self2 {} //~ ERROR: expected contract, found namespace
//~^ ERROR: expected base class, found namespace

contract H is does_not_exist {}
