//@compile-flags: -Ztypeck
// check-fail

import {DefinedInt} from "./auxiliary/defined_non_global_operator.sol";

contract C {
    function binary(DefinedInt a, DefinedInt b) public pure returns (DefinedInt) {
        return a + b;
    }

    function unary(DefinedInt a) public pure returns (DefinedInt) {
        return -a;
    }
}
