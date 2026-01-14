//@ compile-flags: -Ztypeck

abstract contract A {}
contract C {
    function f() public pure {
        new A(); //~ ERROR: cannot instantiate abstract contracts
    }
}
