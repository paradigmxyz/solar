//@ compile-flags: -Ztypeck

contract C {
    uint a = msg(1000); //~ ERROR: expected function, found `msg`
}
