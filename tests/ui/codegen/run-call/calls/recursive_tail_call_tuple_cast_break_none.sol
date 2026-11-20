//@ compile-flags: -Onone
//@ run-call: sum 5000 => 7

// A recursive tail call that returns a tuple stays a tail call at `-Onone` when a field passes
// through a pointer cast, even with a named `break` between the call and the return: a frame per
// level would overflow the EVM stack.
contract RecursiveTailCallTupleCastBreak {
    function first(uint256 n) internal pure returns (bytes memory, uint256) {
        if (n == 0) return (new bytes(1), 7);
        bytes memory p;
        uint256 b;
        for (;;) {
            uint256 w;
            (w, b) = second(n - 1);
            assembly {
                p := w
            }
            break;
        }
        return (p, b);
    }

    function second(uint256 n) internal pure returns (uint256, uint256) {
        (bytes memory p, uint256 b) = first(n);
        uint256 w;
        assembly {
            w := p
        }
        return (w, b);
    }

    function sum(uint256 n) external pure returns (uint256) {
        (, uint256 b) = first(n);
        return b;
    }
}
