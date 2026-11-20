//@ compile-flags: -Onone
//@ run-call: len 0x0102, 5000 => 2

// A recursive tail call that returns a calldata slice stays a tail call at `-Onone` when assembly
// makes a temporary slice that nothing reads, even with a named `break` between the call and the
// return: a frame per level would overflow the EVM stack.
contract RecursiveTailCallTemporarySlice {
    function rec(bytes calldata d, uint256 n) internal pure returns (bytes calldata) {
        if (n == 0) return d;
        bytes calldata r = d;
        for (;;) {
            r = rec(d, n - 1);
            bytes calldata t = r;
            assembly {
                t.offset := 0
            }
            break;
        }
        return r;
    }

    function len(bytes calldata d, uint256 n) external pure returns (uint256) {
        return rec(d, n).length;
    }
}
