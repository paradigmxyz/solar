//@ compile-flags: -Onone
//@ run-call: sum 5000 => 0

// A recursive tail call stays a tail call at `-Onone` when assembly reads the environment and
// discards the value between the call and the named `break`: the read emits no code.
contract RecursiveTailCallEnvironmentRead {
    function rec(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 0;
        uint256 r;
        for (;;) {
            r = rec(n - 1);
            assembly {
                pop(calldatasize())
            }
            break;
        }
        return r;
    }

    function sum(uint256 n) external pure returns (uint256) {
        return rec(n);
    }
}
