//@compile-flags: -Ztypeck
// check-fail

import "./auxiliary/non_global_left.sol";
import "./auxiliary/non_global_right.sol";

contract C {
    function binary(TransitiveInt a, TransitiveInt b) public pure returns (TransitiveInt) {
        return a + b;
    }

    function unary(TransitiveInt a) public pure returns (TransitiveInt) {
        return -a;
    }
}
