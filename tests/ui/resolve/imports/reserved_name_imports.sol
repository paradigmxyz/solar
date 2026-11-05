import "./reserved_name_imports.sol" as _; //~ ERROR: the name `_` is reserved
import {C as this} from "./reserved_name_imports.sol"; //~ ERROR: the name `this` is reserved
import {super} from "./reserved_name_imports.sol";

contract C {}

event super();
