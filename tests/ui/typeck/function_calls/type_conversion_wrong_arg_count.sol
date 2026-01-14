//@ compile-flags: -Ztypeck
contract test {
    function f() public {
        uint(1, 1); //~ ERROR: expected exactly one unnamed argument
    }
}
