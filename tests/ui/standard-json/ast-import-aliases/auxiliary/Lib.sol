import { Cycle } from "../Cycle.sol";
import "./Id.sol" as Plain;
import * as Glob from "./Id.sol";

library Lib {
    function id(Plain.Id x) internal pure returns (Glob.Id) {
        return x;
    }
}
