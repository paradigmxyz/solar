import { C } from "../import_module_alias_cycle.sol";
import "./udvt.sol" as Plain;
import * as Glob from "./udvt.sol";

library Lib {
    function id(Plain.MyUdvt x) internal pure returns (Glob.MyUdvt) {
        return x;
    }
}
