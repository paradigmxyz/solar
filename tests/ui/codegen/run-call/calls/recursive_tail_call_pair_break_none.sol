//@ compile-flags: -Onone
//@ run-call: sum 5000 => 0, 0

// A recursive tail call that returns two values stays a tail call at `-Onone`, even when a named
// `break` sits between the call and the return: a frame per level would overflow the EVM stack.
contract PairTailCallBreak {
    function rec(uint256 n) internal pure returns (uint256, uint256) {
        if (n == 0) return (0, 0);
        uint256 a;
        uint256 b;
        for (;;) {
            (a, b) = rec(n - 1);
            break;
        }
        return (a, b);
    }

    function sum(uint256 n) external pure returns (uint256, uint256) {
        return rec(n);
    }
}
