//@ codegen-matrix: standard
//@ run-call: pin => 1
//@ run-call: threeStates 0 => 0
//@ run-call: threeStates 4 => 1
//@ run-call: threeStates 6 => 2
//@ run-call: threeStates 12 => 3
//@ run-call: threeStates 30 => 3
//@ run-call: fourStates 0 => 0
//@ run-call: fourStates 4 => 1
//@ run-call: fourStates 6 => 2
//@ run-call: fourStates 12 => 3
//@ run-call: fourStates 30 => 0

// A loop whose body is a chain of `else if` arms over the loop-carried state.
// The counter is dropped from the stack inside the chain, so the arms and the
// merge blocks that rejoin the latch reach it through its frame slot. Which
// block owns that store depends on the emission order: some arms are emitted
// after the merge block they jump to, and some after their own predecessors,
// so the counter's home is sometimes already written and sometimes still owed
// by a predecessor further down the stream.
contract LoopStateChainSpillHome {
    function pin() external pure returns (uint256) {
        return 1;
    }

    function threeStates(uint256 n) external pure returns (uint256) {
        uint256 state = 0;
        for (uint256 i = 0; i < n; i++) {
            if (state == 0) {
                if (i == 3) {
                    state = 1;
                }
            } else if (state == 1) {
                if (i == 5) {
                    state = 2;
                }
            } else if (state == 2) {
                state = 3;
            }
        }
        return state;
    }

    function fourStates(uint256 n) external pure returns (uint256) {
        uint256 state = 0;
        for (uint256 i = 0; i < n; i++) {
            if (state == 0) {
                if (i == 3) {
                    state = 1;
                }
            } else if (state == 1) {
                if (i == 5) {
                    state = 2;
                }
            } else if (state == 2) {
                if (i == 11) {
                    state = 3;
                }
            } else if (state == 3) {
                if (i == 13) {
                    state = 0;
                }
            }
        }
        return state;
    }
}
