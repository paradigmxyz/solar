//@compile-flags: -Ztypeck
// Test: pure function cannot read state variables

contract C {
    uint256 public x;

    function pureReadsState() public pure returns (uint256) {
        return x;
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires "view"
    }
}
