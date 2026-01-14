//@ compile-flags: -Ztypeck

contract C {
    function f(bool b) public pure {
        uint(b); //~ ERROR: invalid explicit type conversion
    }
}
