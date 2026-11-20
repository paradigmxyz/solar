//@ compile-flags: -Onone
//@ run-call: sum 5000 => 0

// A recursive tail call stays a tail call at `-Onone` when an assembly block reads a memory
// pointer between the call and the named `break`: the pointer cast emits no code.
contract RecursiveTailCallPointerCast {
    function rec(uint256 n, bytes memory p) internal pure returns (uint256) {
        if (n == 0) return 0;
        uint256 r;
        for (;;) {
            r = rec(n - 1, p);
            assembly {
                pop(p)
            }
            break;
        }
        return r;
    }

    function sum(uint256 n) external pure returns (uint256) {
        return rec(n, new bytes(1));
    }
}
