// solc reports cyclic constants before its static analysis and storage layout checks, so their
// errors do not hide the cycle.
uint constant A = B; //~ ERROR: the value of the constant `A` has a cyclic dependency via `B`
uint constant B = A; //~ ERROR: the value of the constant `B` has a cyclic dependency via `A`
uint constant Z = 0;
uint constant X = Y; //~ ERROR: the value of the constant `X` has a cyclic dependency via `Y`
uint constant Y = X; //~ ERROR: the value of the constant `Y` has a cyclic dependency via `X`

contract C layout at X {
    function f() public pure returns (uint) {
        return 1 / Z;
    }
}
