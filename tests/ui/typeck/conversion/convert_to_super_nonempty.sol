//@compile-flags: -Ztypeck
contract C {
    function f() public pure {
        super(this).f(); //~ ERROR: expected function, found `contract C`
    }
}
