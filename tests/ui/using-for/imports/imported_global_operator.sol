// ported-from: test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported.sol

import {Int} from "./auxiliary/imported_using.sol";

contract C {
    function f(Int a, Int b) public pure returns (Int) {
        return a + b;
    }
}
