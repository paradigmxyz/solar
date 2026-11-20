//@ compile-flags: -Onone
//@ run-call: sum 5000 => 12502500

// `-Onone` folds no call into a tail call, except a call into the function's own recursive
// component: a frame per level would overflow the EVM stack long before 5000 levels.
contract RecursiveTailCall {
    function rec(uint256 n, uint256 acc) internal pure returns (uint256) {
        if (n == 0) return acc;
        return rec(n - 1, acc + n);
    }

    function sum(uint256 n) external pure returns (uint256) {
        return rec(n, 0);
    }
}
