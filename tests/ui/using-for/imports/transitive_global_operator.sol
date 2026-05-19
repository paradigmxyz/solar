//@compile-flags: -Ztypeck
// Ported from test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported.sol.

import "./auxiliary/transitive_mid.sol";

contract C {
    function f(Int a, Int b) public pure returns (Int, Int) {
        return (a + b, -a);
    }
}
