function inc(uint256 x) pure returns (uint256) {
    return x + 1;
}

using {f} for S global;

struct S {
    uint256 x;
}

function gen() pure returns (S memory) {
    return S(1);
}

function f(S memory s) pure returns (uint256) {
    return s.x;
}

function f1(S memory s) pure returns (uint256) {
    return s.x + 1;
}
