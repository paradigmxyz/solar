// Ported from test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported_non_global.sol.

type ImportedInt is int256;

function add(ImportedInt a, ImportedInt b) pure returns (ImportedInt) {
    return ImportedInt.wrap(ImportedInt.unwrap(a) + ImportedInt.unwrap(b));
}

function neg(ImportedInt a) pure returns (ImportedInt) {
    return ImportedInt.wrap(-ImportedInt.unwrap(a));
}
