//@compile-flags: -Ztypeck
// Test: constant state variables can be accessed from pure functions

contract C {
    uint constant x = 2;

    function k() public pure returns (uint) {
        return x;
    }
}
