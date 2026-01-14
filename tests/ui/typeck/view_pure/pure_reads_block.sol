//@compile-flags: -Ztypeck
// Test: pure function cannot read block.* variables

contract C {
    function pureReadsBlock() public pure returns (uint256) {
        return block.timestamp;
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires "view"
    }

    function pureReadsBlockNumber() public pure returns (uint256) {
        return block.number;
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires "view"
    }
}
