// Solc tests:
// - test/libsolidity/syntaxTests/using/module_2.sol.
// - test/libsolidity/syntaxTests/using/module_3.sol.
// - test/libsolidity/syntaxTests/using/library_import_as.sol.
// - test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported.sol.

type Int is int256;

function add(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

function inc(uint256 x) pure returns (uint256) {
    return x + 1;
}

library Lib {
    function twice(uint256 x) internal pure returns (uint256) {
        return x * 2;
    }
}

using {add as +} for Int global;
