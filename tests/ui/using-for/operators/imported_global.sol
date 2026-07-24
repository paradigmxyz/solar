// ported-from: test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported.sol

import {Int} from "./auxiliary/operator_imported.sol";

contract C {
    function f(Int a, Int b) public pure returns (Int, Int) {
        return (a + b, -a);
    }
}
