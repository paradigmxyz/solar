using {f} for S global;
using {f} for S;

struct S {
    uint256 x;
}

function gen() pure returns (S memory) {}

function f(S memory x) pure returns (uint256) {
    return x.x;
}
