// Ported from test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported_transitively.sol.

import "./global_wrong_type.sol";

function add2(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

function unsub2(Int a) pure returns (Int) {
    return Int.wrap(-Int.unwrap(a));
}

using {add2 as +} for Int global;
using {unsub2 as -} for Int global;
