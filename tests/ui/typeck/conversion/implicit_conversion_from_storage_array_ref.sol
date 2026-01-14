//@compile-flags: -Ztypeck
// Solar rejects this (unlike solc)
contract C {
    int[10] x;
    int[] y;
    function f() public {
        y = x; //~ ERROR: mismatched types
    }
}
