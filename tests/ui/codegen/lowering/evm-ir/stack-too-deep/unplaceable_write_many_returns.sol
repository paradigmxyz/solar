//@ codegen-matrix: standard
//@ run-call: f 1 => 12
//@ run-call: f 5 => 20
//@ run-call: k 0 => 0
//@ run-call: k 1 => 2
//@ run-call: k 3 => 6

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

    // Only the deepest of the 18 results is used. `h` is recursive, so the parameters stay
    // spilled across the call, and the result is spilled in place under the dead ones.
    function h(uint256 p0, uint256 p1, uint256 p2, uint256 p3, uint256 p4, uint256 p5, uint256 p6, uint256 p7, uint256 p8, uint256 p9, uint256 p10, uint256 p11, uint256 p12, uint256 p13, uint256 p14, uint256 p15) internal pure returns (uint256) {
        if (p0 == 0) return p11;
        (, , , , , , , , , , , , , , , , , uint256 a17) = u(p1);
        return a17 + h(p0 - 1, 1, 1, 1, p11, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1);
    }

    function u(uint256 p) internal pure returns (uint256 r0, uint256 r1, uint256 r2, uint256 r3, uint256 r4, uint256 r5, uint256 r6, uint256 r7, uint256 r8, uint256 r9, uint256 r10, uint256 r11, uint256 r12, uint256 r13, uint256 r14, uint256 r15, uint256 r16, uint256 r17) {
        w(p);
        r17 = p;
    }

    function k(uint256 x) external pure returns (uint256) {
        return h(x, x, x, x, x, x, x, x, x, x, x, x, x, x, x, x);
    }
}
