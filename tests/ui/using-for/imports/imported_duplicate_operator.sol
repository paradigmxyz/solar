// ported-from: test/libsolidity/syntaxTests/operators/userDefined/multiple_operator_definitions_different_functions_global_and_non_global_different_files.sol

import {Int} from "./auxiliary/transitive_base.sol";

function add2(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

using {add2 as +} for Int; //~ ERROR: operators can only be defined in a global
//~^ ERROR: user-defined binary operator `+` has more than one definition
