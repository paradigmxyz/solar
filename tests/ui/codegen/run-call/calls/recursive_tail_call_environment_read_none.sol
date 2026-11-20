//@ compile-flags: -Onone
//@ run-call: sum 5000 => 0

// A recursive tail call stays a tail call at `-Onone` when assembly reads the environment, as
// `calldatasize` and `caller` do, and discards the value between the call and the named `break`:
// the read and its widening emit no code.
contract RecursiveTailCallEnvironmentRead {
    function rec(uint256 n) internal view returns (uint256) {
        if (n == 0) return 0;
        uint256 r;
        for (;;) {
            r = rec(n - 1);
            assembly {
                pop(calldatasize())
                pop(caller())
            }
            uint256(uint176(uint160(msg.sender))); //~ WARN: statement has no effect
            break;
        }
        return r;
    }

    function sum(uint256 n) external view returns (uint256) {
        return rec(n);
    }
}
