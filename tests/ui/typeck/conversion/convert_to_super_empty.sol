//@compile-flags: -Ztypeck
contract C {
    function f() public pure {
        super().x; //~ ERROR: expected function, found `contract C`
    }
}
