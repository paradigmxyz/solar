//@compile-flags: -Ztypeck
// Tests that reading constants is allowed in pure functions.
// Ported from solc viewPureChecker/constant.sol

contract C {
    uint constant x = 2;
    function k() pure public returns (uint) {
        return x;
    }
}
