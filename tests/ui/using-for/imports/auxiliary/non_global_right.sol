// ported-from: test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported_transitively_non_global.sol

import "./non_global_common.sol";

function addRight(TransitiveInt a, TransitiveInt b) pure returns (TransitiveInt) {
    return TransitiveInt.wrap(TransitiveInt.unwrap(a) + TransitiveInt.unwrap(b));
}

function negRight(TransitiveInt a) pure returns (TransitiveInt) {
    return TransitiveInt.wrap(-TransitiveInt.unwrap(a));
}

using {addRight as +, negRight as -} for TransitiveInt;
