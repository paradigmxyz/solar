//@ codegen-matrix: standard
//@ run-call: pick 0, 7 => 0
//@ run-call: pick 1, 7 => 8
//@ run-call: pick 2, 9 => 10

//@ run-call: comparisons 0, 0 => true, false, 11
//@ run-call: comparisons 7, 7 => true, false, 11
//@ run-call: comparisons 0, 7 => false, true, 22
//@ run-call: comparisons 7, 0 => false, true, 22
//@ run-call: comparisons 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => false, true, 22
//@ run-call: zeros 0 => true, false, 11
//@ run-call: zeros 1 => false, true, 22
//@ run-call: zeros 2 => false, true, 22
//@ run-call: zeros 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => false, true, 22

// A branch condition that stays live past its JUMPI must be spilled before a
// preserved-stack edge, or the successor reloads a never-written slot.
contract BranchConditionLiveOut {
    function comparisons(uint256 a, uint256 b) public pure returns (bool equal, bool different, uint256 result) {
        equal = a == b;
        different = b != a;
        if (equal) { result = 1; } else { result = 2; }
        if (different) { result += 20; } else { result += 10; }
    }

    function zeros(uint256 value) public pure returns (bool equal, bool different, uint256 result) {
        equal = 0 == value;
        different = value != 0;
        if (equal) { result = 1; } else { result = 2; }
        if (different) { result += 20; } else { result += 10; }
    }

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
