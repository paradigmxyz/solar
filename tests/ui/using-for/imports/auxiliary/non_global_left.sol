// Solc test: test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported_transitively_non_global.sol.

import "./non_global_common.sol";

function addLeft(TransitiveInt a, TransitiveInt b) pure returns (TransitiveInt) {
    return TransitiveInt.wrap(TransitiveInt.unwrap(a) + TransitiveInt.unwrap(b));
}

function negLeft(TransitiveInt a) pure returns (TransitiveInt) {
    return TransitiveInt.wrap(-TransitiveInt.unwrap(a));
}

using {addLeft as +, negLeft as -} for TransitiveInt;
