//@ compile-flags: -Onone
//@ run-call: len 0x0102, 5000 => 2, 7

// A recursive tail call that returns a calldata slice inside a tuple stays a tail call at
// `-Onone` when assembly writes the slice's own offset back, even with a named `break` between the
// call and the return: a frame per level would overflow the EVM stack.
contract RecursiveTailCallNestedSliceRebuild {
    function rec(bytes calldata d, uint256 n) internal pure returns (bytes calldata, uint256) {
        if (n == 0) return (d, 7);
        bytes calldata r = d;
        uint256 w;
        for (;;) {
            (r, w) = rec(d, n - 1);
            assembly {
                r.offset := r.offset
            }
            break;
        }
        return (r, w);
    }

    function len(bytes calldata d, uint256 n) external pure returns (uint256, uint256) {
        (bytes calldata r, uint256 w) = rec(d, n);
        return (r.length, w);
    }
}
