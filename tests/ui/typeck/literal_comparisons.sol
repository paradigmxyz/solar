// Two literals compare in the common type of their mobile types: `-1` is an `int8` and `0` a
// `uint8`, and integer types of different signedness have no common type.

contract C {
    function f() public pure {
        -1 == -2;
        1 < 2;
        1 == 300;
        -1000 > -1;
        -1 < 0; //~ ERROR: cannot apply builtin operator `<`
        -1 != 1; //~ ERROR: cannot apply builtin operator `!=`
        100 >= -200; //~ ERROR: cannot apply builtin operator `>=`
        -1 + 0 < 0; //~ ERROR: cannot apply builtin operator `<`
        -(0.5 * 2) <= 0; //~ ERROR: cannot apply builtin operator `<=`
    }
}
