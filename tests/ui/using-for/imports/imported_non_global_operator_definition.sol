//@compile-flags: -Ztypeck
//@ error-in-other-file: operators can only be defined in a global
//@ error-in-other-file: operators can only be defined in a global

import {DefinedInt} from "./auxiliary/defined_non_global_operator.sol";

contract C {
    function binary(DefinedInt a, DefinedInt b) public pure returns (DefinedInt) {
        return a + b; //~ ERROR: cannot apply builtin operator
    }

    function unary(DefinedInt a) public pure returns (DefinedInt) {
        return -a; //~ ERROR: cannot apply unary operator
    }
}
