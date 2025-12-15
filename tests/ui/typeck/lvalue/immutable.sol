//@compile-flags: -Ztypeck
// TODO: assignments to immutables in the constructor should be allowed
// TODO: `mismatched types` errors on integer literals are a current limitation of solar

contract Test {
    uint256 immutable IMMUT;

    constructor() {
        // This should be OK in constructor but currently errors
        IMMUT = 1; //~ ERROR: cannot assign to an immutable variable
        //~^ ERROR: mismatched types
    }

    function test() external {
        IMMUT = 2; //~ ERROR: cannot assign to an immutable variable
        //~^ ERROR: mismatched types
    }
}
