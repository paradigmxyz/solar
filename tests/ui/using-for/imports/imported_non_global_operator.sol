// ported-from: test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported_non_global.sol

import {ImportedInt} from "./auxiliary/non_global_operator.sol";
import {add, neg} from "./auxiliary/non_global_operator.sol";

using {add as +, neg as -} for ImportedInt;
//~^ ERROR: operators can only be defined in a global
//~| ERROR: operators can only be defined in a global

contract C {
    function binary(ImportedInt a, ImportedInt b) public pure returns (ImportedInt) {
        return a + b;
    }

    function unary(ImportedInt a) public pure returns (ImportedInt) {
        return -a;
    }
}
