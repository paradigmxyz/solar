//@compile-flags: -Ztypeck
//@ error-in-other-file: can only use `global` with types defined in the same source unit at file level
//@ error-in-other-file: can only use `global` with types defined in the same source unit at file level
//@ error-in-other-file: can only use `global` with types defined in the same source unit at file level
//@ error-in-other-file: can only use `global` with types defined in the same source unit at file level
// check-fail
// Ported from test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported_transitively.sol.

import "./auxiliary/global_wrong_left.sol";
import "./auxiliary/global_wrong_right.sol";

contract C {
    function f() public {
        Int.wrap(0) + Int.wrap(0); //~ ERROR: cannot apply builtin operator
        -Int.wrap(0); //~ ERROR: cannot apply unary operator
    }
}
