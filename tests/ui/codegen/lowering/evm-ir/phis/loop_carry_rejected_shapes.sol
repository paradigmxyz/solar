//@ codegen-matrix: standard
//@ run-call: sumEvens 6 => 6
//@ run-call: sumEvens 0 => 0
//@ run-call: countdown 5, 3 => 18
//@ run-call: countdown 0, 3 => 3
//@ run-call: firstAbove 4 => 5

// Loops the loop planner rejects: a body that writes storage, a branch latch
// with a shared exit, and a self loop whose exit is a join. The counters and
// invariants are carried on the stack by the join planner instead of being
// reloaded on every iteration.
contract LoopCarryRejectedShapes {
    uint256 public counter;

    function sumEvens(uint256 n) external returns (uint256 s) {
        for (uint256 i; i < n; ++i) {
            if (i % 2 == 0) {
                s += i;
                counter += 1;
            }
        }
    }

    function countdown(uint256 n, uint256 step) external pure returns (uint256 acc) {
        uint256 i = n;
        do {
            acc += i;
            if (i == 0) break;
            i -= 1;
        } while (true);
        acc += step;
    }

    function firstAbove(uint256 limit) external pure returns (uint256 i) {
        while (i <= limit) {
            i += 1;
        }
    }
}
