// Underscores next to an uppercase `E` are valid, so fractional values reach type checking.
contract C {
    function f() public pure returns (uint) {
        return 15_E-1; //~ ERROR: mismatched types
    }
}
