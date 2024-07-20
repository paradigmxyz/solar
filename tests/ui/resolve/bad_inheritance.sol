import "./bad_inheritance.sol" as self1;
import * as self2 from "./bad_inheritance.sol";
import {does_not_exist} from "./bad_inheritance.sol"; //~ ERROR unresolved import

contract C is C {} //~ ERROR contracts cannot inherit from themselves
contract D is self1.D {} //~ ERROR contracts cannot inherit from themselves
contract E is self2.E {} //~ ERROR contracts cannot inherit from themselves

contract F is self1 {} //~ ERROR expected contract
contract G is self2 {} //~ ERROR expected contract

contract H is does_not_exist {}
