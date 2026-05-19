// Solc test: test/libsolidity/syntaxTests/operators/userDefined/calling_operator_imported.sol.

type Int is int256;

function add(Int a, Int b) pure returns (Int) {
    return Int.wrap(Int.unwrap(a) + Int.unwrap(b));
}

function neg(Int a) pure returns (Int) {
    return Int.wrap(-Int.unwrap(a));
}

using {add as +, neg as -} for Int global;
