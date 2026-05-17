//@compile-flags: -Ztypeck

type U is uint256;

using {add as +, neg as -, eq as ==} for U global;

function add(U a, U b) pure returns (U) {
    return U.wrap(U.unwrap(a) + U.unwrap(b));
}

function neg(U a) pure returns (U) {
    return U.wrap(0 - U.unwrap(a));
}

function eq(U a, U b) pure returns (bool) {
    return U.unwrap(a) == U.unwrap(b);
}

contract C {
    function f(U a, U b) public pure {
        U c = a + b;
        U d = -a;
        bool e = a == b;
    }
}
