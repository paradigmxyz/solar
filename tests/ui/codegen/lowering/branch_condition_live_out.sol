//@ compile-flags: -Ogas
//@ run-call: pick 0, 7 => 0
//@ run-call: pick 1, 7 => 8
//@ run-call: pick 2, 9 => 10

// A branch condition that stays live past its JUMPI must be spilled before a
// preserved-stack edge, or the successor reloads a never-written slot.
contract BranchConditionLiveOut {
    function pick(uint256 flag, uint256 x) public pure returns (uint256) {
        bool keep = flag != 0;
        uint256 acc = 0;
        if (keep) {
            acc = x;
        }
        uint256 bump = acc + 1;
        if (keep) {
            acc = bump;
        }
        return acc;
    }
}
