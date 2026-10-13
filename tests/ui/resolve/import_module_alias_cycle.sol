// The auxiliary source sorts first, so the cycle places this source before it and
// its module aliases must exist before its imports are performed.
import { Lib, Plain, Glob } from "./auxiliary/module_alias_cycle.sol";

contract C {
    function f(Plain.MyUdvt x) external pure returns (Glob.MyUdvt) {
        return Lib.id(x);
    }
}
