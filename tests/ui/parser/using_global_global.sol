struct S {
    uint256 x;
}

function f(S memory) pure {}

using {f} for S global global; //~ ERROR: expected
