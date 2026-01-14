//@compile-flags: -Ztypeck
contract C {
    int[10] x;
    function f() public view {
        int[](x); //~ ERROR: invalid explicit type conversion
        int(x); //~ ERROR: invalid explicit type conversion
    }
}
