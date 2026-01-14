//@ compile-flags: -Ztypeck

contract C {
    function f() public pure {
        int8(256); //~ ERROR: invalid explicit type conversion
        uint8(256); //~ ERROR: invalid explicit type conversion
    }
}
