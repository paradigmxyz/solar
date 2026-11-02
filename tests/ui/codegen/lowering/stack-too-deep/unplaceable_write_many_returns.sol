//@ codegen-matrix: standard
//@ run-call: f 1 => 12
//@ run-call: f 5 => 20

contract UnplaceableWriteManyReturns {
    // The write in `w` is not memory-safe, so a call to `g` may overwrite the spill area and `f`
    // saves the spilled words on the stack across it. The 18 results land above them, and the
    // dead ones are dropped to restore the saved words.
    function w(uint256 p) internal pure {
        assembly {
            mstore(p, p)
        }
    }

    function g(uint256 p) internal pure returns (uint256 r0, uint256 r1, uint256 r2, uint256 r3, uint256 r4, uint256 r5, uint256 r6, uint256 r7, uint256 r8, uint256 r9, uint256 r10, uint256 r11, uint256 r12, uint256 r13, uint256 r14, uint256 r15, uint256 r16, uint256 r17) {
        w(p);
        r2 = p + 2;
        r3 = p + 3;
        r5 = p + 5;
    }

    function f(uint256 x) external pure returns (uint256) {
        (, , , uint256 a3, , uint256 a5, , , , , , , , , , , , ) = g(x);
        (, , uint256 b2, , , , , , , , , , , , , , , ) = g(a3);
        return a5 + b2;
    }
}
