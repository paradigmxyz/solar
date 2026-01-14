//@compile-flags: -Ztypeck
contract C {
    uint immutable x = 0;

    function f() internal {
        x = 1; //~ ERROR: cannot assign to an immutable variable
    }
}
