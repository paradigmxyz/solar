type DefinedInt is int256;

function add(DefinedInt a, DefinedInt b) pure returns (DefinedInt) {
    return DefinedInt.wrap(DefinedInt.unwrap(a) + DefinedInt.unwrap(b));
}

function neg(DefinedInt a) pure returns (DefinedInt) {
    return DefinedInt.wrap(-DefinedInt.unwrap(a));
}

using {add as +, neg as -} for DefinedInt;
