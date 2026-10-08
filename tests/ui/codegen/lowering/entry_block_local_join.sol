//@ codegen-matrix: standard
//@ run-call: a 1
//@ run-call: c 2
//@ run-call: b => 0
//@ run-call-fail: a 1; value=1

// The two empty functions share the dispatcher's `stop` block, a join in an
// entry whose values are all block-local. Its liveness tracks no value across
// blocks, but its rows must still span every value for the join planner.
contract EntryBlockLocalJoin {
    function a(uint256) external pure {}

    function c(uint256) external pure {}

    function b() external pure returns (uint256) {
        return 0;
    }
}
