//@ codegen-matrix: standard
//@ run-call: 0x94209e430000000000000000000000000000000000000000000000000000000000000000 => 0x
//@ run-call: outer 1 => 8, 10
//@ run-call: outer 3 => 8, 10
//@ run-call: 0xc715a02a0000000000000000000000000000000000000000000000000000000000000000 => 0x

// A zero-length assembly return terminates the whole invocation, including a caller that
// otherwise expects two internal results. Recursion keeps the shared body observable.
contract ValueReturningStop {
    function inner(uint256 mode) public pure returns (uint256, uint256) {
        if (mode == 0) {
            assembly {
                return(0, 0)
            }
        }
        if (mode == 1) return (7, 9);
        return inner(mode - 1);
    }

    function outer(uint256 mode) external pure returns (uint256, uint256) {
        (uint256 a, uint256 b) = inner(mode);
        return (a + 1, b + 1);
    }
}
