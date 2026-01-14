//@ compile-flags: -Ztypeck

interface I {}
contract C {
    function f() public pure {
        new I(); //~ ERROR: cannot instantiate interfaces
    }
}
