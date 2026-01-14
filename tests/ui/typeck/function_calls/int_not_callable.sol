//@ compile-flags: -Ztypeck

contract C {
    function f() public {
        (1(3), 2); //~ ERROR: expected function, found `int_literal[1]`
    }
}
