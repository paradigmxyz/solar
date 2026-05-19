//@compile-flags: -Ztypeck
//@ error-in-other-file: can only use `global` with types defined in the same source unit at file level
//@ error-in-other-file: can only use `global` with types defined in the same source unit at file level
// check-fail

import "./auxiliary/global_wrong_left.sol";
import "./auxiliary/global_wrong_right.sol";

contract C {
    function binary(WrongSourceInt a, WrongSourceInt b) public pure returns (WrongSourceInt) {
        return a + b; //~ ERROR: cannot apply builtin operator
    }

    function unary(WrongSourceInt a) public pure returns (WrongSourceInt) {
        return -a; //~ ERROR: cannot apply unary operator
    }
}
