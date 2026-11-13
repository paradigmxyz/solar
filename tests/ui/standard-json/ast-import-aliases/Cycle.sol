import { Lib, Plain, Glob } from "./auxiliary/Lib.sol";

contract Cycle {
    function f(Plain.Id x) external pure returns (Glob.Id) {
        return Lib.id(x);
    }
}
