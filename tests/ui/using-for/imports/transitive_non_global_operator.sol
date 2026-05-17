//@compile-flags: -Ztypeck
//@ error-in-other-file: operators can only be defined in a global
//@ error-in-other-file: operators can only be defined in a global
//@ error-in-other-file: operators can only be defined in a global
//@ error-in-other-file: operators can only be defined in a global

import "./auxiliary/non_global_left.sol";
import "./auxiliary/non_global_right.sol";

contract C {
    function binary(TransitiveInt a, TransitiveInt b) public pure returns (TransitiveInt) {
        return a + b; //~ ERROR: cannot apply builtin operator
    }

    function unary(TransitiveInt a) public pure returns (TransitiveInt) {
        return -a; //~ ERROR: cannot apply unary operator
    }
}
