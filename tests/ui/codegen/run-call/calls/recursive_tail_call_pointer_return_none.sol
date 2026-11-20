//@ compile-flags: -Onone
//@ run-call: len 5000 => 1

// A recursive tail call stays a tail call at `-Onone` when the function returns a pointer that
// casts the call's result: the cast emits no code.
contract RecursiveTailCallPointerReturn {
    function first(uint256 n) internal pure returns (bytes memory r) {
        if (n == 0) return new bytes(1);
        for (;;) {
            uint256 word = second(n - 1);
            assembly {
                r := word
            }
            break;
        }
    }

    function second(uint256 n) internal pure returns (uint256 word) {
        bytes memory r = first(n);
        assembly {
            word := r
        }
    }

    function len(uint256 n) external pure returns (uint256) {
        return first(n).length;
    }
}
