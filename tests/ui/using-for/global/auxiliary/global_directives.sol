struct S {
    uint256 x;
}

type U is uint256;

function sValue(S memory s) pure returns (uint256) {
    return s.x;
}

function unwrap(U u) pure returns (uint256) {
    return U.unwrap(u);
}

using {sValue} for S global;
using {unwrap} for U global;
