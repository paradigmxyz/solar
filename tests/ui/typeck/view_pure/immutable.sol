//@compile-flags: -Ztypeck
// Tests that reading immutables is allowed in pure functions.
// Ported from solc viewPureChecker/immutable.sol

contract B {
    uint immutable x = 1;
    function f() public pure returns (uint) {
        return x;
    }
}
