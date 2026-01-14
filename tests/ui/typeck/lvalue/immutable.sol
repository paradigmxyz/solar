//@compile-flags: -Ztypeck
// TODO: assignments to immutables in the constructor should be allowed

contract Test {
    uint256 immutable IMMUT;

    constructor() {
        // This should be OK in constructor but currently errors
        IMMUT = 1; //~ ERROR: cannot assign to an immutable variable
    }

    function test() external { //~ WARN: function state mutability can be restricted to view
        IMMUT = 2; //~ ERROR: cannot assign to an immutable variable
    }
}
