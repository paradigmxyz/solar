//@ codegen-matrix: standard
//@ run-call: sum 5 => 8
//@ run-call: sum 6 => 18
//@ run-call: sum 0 => 0
//@ run-call: pick 3, 4 => 4
//@ run-call: pick 2, 5 => 18

// The loop counter is a header phi carried on the stack into the arm, which
// follows the predecessor's stack order. The arm needs the counter in memory
// across the call and after it, so a copy stored during an earlier iteration
// must not be reused: entering the arm on a carried stack invalidates the phi
// and the arm stores the current definition on its path. `pick` carries a
// value defined before the loop next to the phis.
contract JoinCarryPhiArmStore {
    // Recursive, so the inliner leaves the call in place.
    function helper(uint256 n, uint256 f) internal pure returns (uint256) {
        if (n == 0) return f;
        return helper(n - 1, f + 1);
    }

    function sum(uint256 n) external pure returns (uint256 acc) {
        for (uint256 i = 0; i < n; i++) {
            if (i % 2 == 1) {
                acc = helper(i, acc) + i;
            }
        }
    }

    function pick(uint256 a, uint256 b) external pure returns (uint256 r) {
        uint256 f = a > b ? a - b : b - a;
        for (uint256 i = 0; i < b; i++) {
            if (i >= a) {
                r += helper(f, i);
            }
        }
    }
}
