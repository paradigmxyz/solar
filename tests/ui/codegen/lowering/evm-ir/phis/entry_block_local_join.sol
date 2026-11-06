//@ codegen-matrix: standard
//@ run-call: a 1
//@ run-call: c 2
//@ run-call: b => 0
//@ run-call: d => 1
//@ run-call: 0x8a054ac200 => 0x0000000000000000000000000000000000000000000000000000000000000002
//@ run-call: 0x8a054ac20000000000000000000000000000000000000000000000000000000000000000 => 0x0000000000000000000000000000000000000000000000000000000000000003
//@ run-call-fail: a 1; value=1

// The two empty functions share the dispatcher's `stop` block, a join in an
// entry whose values are all block-local. Its liveness tracks no value across
// blocks, but its rows must still span every value for the join planner.
// `d` adds a join whose phi takes only constants.
contract EntryBlockLocalJoin {
    function a(uint256) external pure {}

    function c(uint256) external pure {}

    function b() external pure returns (uint256) {
        return 0;
    }

    function d() external pure returns (uint256 r) {
        assembly {
            switch calldatasize()
            case 4 { r := 1 }
            case 5 { r := 2 }
            default { r := 3 }
        }
    }
}
