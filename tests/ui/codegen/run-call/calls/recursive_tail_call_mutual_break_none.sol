//@ compile-flags: -Onone
//@ run-call: sum 5000 => 0

// Mutually recursive tail calls stay tail calls at `-Onone`, even when a named `break` sits
// between the call and the return: a frame per level would overflow the EVM stack.
contract MutualTailCallBreak {
    function first(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 0;
        uint256 r;
        for (;;) {
            r = second(n - 1);
            break;
        }
        return r;
    }

    function second(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 0;
        return first(n - 1);
    }

    function sum(uint256 n) external pure returns (uint256) {
        return first(n);
    }
}
