// ported-from: test/libsolidity/syntaxTests/operators/userDefined/defining_operator_for_enum.sol
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/defining_operator_for_struct.sol

enum E {
    A,
    B
}

struct S {
    uint256 x;
}

function addE(E a, E b) pure returns (E) {
    b;
    return a;
}

function addS(S memory a, S memory b) pure returns (S memory) {
    b;
    return a;
}

using {addE as +} for E global; //~ ERROR: operators can only be implemented for user-defined value types
using {addS as +} for S global; //~ ERROR: operators can only be implemented for user-defined value types
