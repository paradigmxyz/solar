//@compile-flags: -Ztypeck
//@ error-in-other-file: operators can only be defined in a global
//@ error-in-other-file: operators can only be defined in a global
//@ error-in-other-file: operators can only be defined in a global
//@ error-in-other-file: operators can only be defined in a global
// check-fail
// Ported from test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported_transitively_non_global.sol.

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
