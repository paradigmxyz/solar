//@ compile-flags: -Ztypeck

library L {}
contract C {
    function f() public pure {
        new L(); //~ ERROR: cannot instantiate librarys
    }
}
