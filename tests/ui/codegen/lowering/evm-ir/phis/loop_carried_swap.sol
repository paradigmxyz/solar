//@ codegen-matrix: standard
//@ compile-flags: -Zvalidate-ir=true
//@ run-call: swapUnused 1, 2, 5 => 7
//@ run-call: swapUntilSet 1, 2, 5 => 7

// Swapped loop values that nothing reads after the loop form a cycle of header phis. Jump
// threading must not bypass the header, whose phis would then carry undefined values.
contract LoopCarriedSwap {
    function swapUnused(uint256 a, uint256 b, uint256 n) external pure returns (uint256) {
        uint256 i = 0;
        while (i == 0) {
            (a, b) = (b, a);
            i = n;
        }
        return 7;
    }

    function swapUntilSet(uint256 a, uint256 b, uint256 n) external pure returns (uint256) {
        bool done = false;
        while (!done) {
            (a, b) = (b, a);
            done = n > 3;
        }
        return 7;
    }
}
