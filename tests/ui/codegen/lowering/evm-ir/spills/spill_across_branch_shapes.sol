//@ codegen-matrix: standard
//@ run-call: singleArmCall 0, 1, 0 => 0x1cd913cbd0697182a8
//@ run-call: singleArmCall 1, 0, 2 => 0x8b
//@ run-call: singleArmCall 0, 0, 0 => 0x4b
//@ run-call: singleArmCall 1, 1, 1 => 0x73644f2f41a5c60b0
//@ run-call: branchInLoop 0, 1, 0 => 0x92acc969c11045de08ff7d65a0db80598
//@ run-call: branchInLoop 1, 0, 2 => 0x379f80cb36c02f9fc3
//@ run-call: branchInLoop 0, 0, 0 => 0x215fb3ad20d9b62c15
//@ run-call: branchInLoop 1, 1, 1 => 0x73644f2f41a5c60b0
//@ run-call: earlyReturn 0, 1, 0 => 0x278dde6e5fd29f0557
//@ run-call: earlyReturn 1, 0, 2 => 0x8a
//@ run-call: earlyReturn 0, 0, 0 => 0x6b
//@ run-call: earlyReturn 1, 1, 1 => 0x73644f2f41a5c60a98
//@ run-call: twoBranchLoops 0, 1, 0 => 0xe947b6b2d9b06e76646f266b0181bc607f
//@ run-call: twoBranchLoops 1, 0, 2 => 0x3c913c9902ba838087
//@ run-call: twoBranchLoops 0, 0, 0 => 0x7947b6b2d9b06e48c3
//@ run-call: twoBranchLoops 1, 1, 1 => 0x1d089ba4b236670907cb112c1550f259e7
//@ run-call: whileTrueThenLoop 0, 1, 0 => 0x39b22797a0d2e30426
//@ run-call: whileTrueThenLoop 1, 0, 2 => 0xb6
//@ run-call: whileTrueThenLoop 0, 0, 0 => 0x83
//@ run-call: whileTrueThenLoop 1, 1, 1 => 0xe6c89e5e834b8c179


// More layout shapes where a block is emitted before one of its predecessors,
// so the predecessor owes the frame store for every value the successor drops
// from the stack. Each function keeps more values live across the branch and
// the loop than any carried stack layout holds, and the two arms redefine
// different subsets of them.
contract SpillAcrossBranchShapes {
    function step(uint256 x, uint256 y) internal pure returns (uint256, uint256) {
        unchecked {
            return (x * 3 + y, x ^ (y << 1));
        }
    }

    function mix(uint256 x) internal pure returns (uint256) {
        unchecked {
            return x * 0x9e3779b97f4a7c15 + 1;
        }
    }

    // Only the `else` arm calls out, so only it spills; the `if` arm hands its
    // values to the join on the stack.
    function singleArmCall(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1; uint256 v1 = c + 2; uint256 v2 = b + 3;
            uint256 v3 = v1 - v2; uint256 v4 = v0 + v1; uint256 v5 = v1 ^ v2;
            uint256 v6 = v5 ^ v1; uint256 v7 = v3 + v6;
            if (v2 & 1 == 1) { v7 = v6 * v4; } else { v2 = mix(v6); v3 = mix(v5); }
            for (uint256 i = 0; i < (v4 & 3); i++) { (v3, v2) = step(v1, v2); v7 = v7 ^ i; }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8);
        }
    }

    // The loop header joins three edges: both branch arms and the latch.
    function branchInLoop(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1; uint256 v1 = c + 2; uint256 v2 = b + 3;
            uint256 v3 = v1 - v2; uint256 v4 = v0 + v1; uint256 v5 = v1 ^ v2;
            uint256 v6 = v5 ^ v1; uint256 v7 = v3 + v6;
            if (v2 & 1 == 1) { v7 = v6 * v4; } else { v2 = mix(v6); }
            for (uint256 i = 0; i < (v4 & 3); i++) {
                if (i & 1 == 0) { (v3, v2) = step(v1, v2); } else { v5 = mix(v3); }
            }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8);
        }
    }

    // An early return from inside the loop adds a second exit whose block is
    // laid out before the arms that reach it.
    function earlyReturn(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1; uint256 v1 = c + 2; uint256 v2 = b + 3;
            uint256 v3 = v1 - v2; uint256 v4 = v0 + v1; uint256 v5 = v1 ^ v2;
            uint256 v6 = v5 ^ v1; uint256 v7 = v3 + v6;
            if (v2 & 1 == 1) { v7 = v6 * v4; } else { v2 = mix(v6); }
            for (uint256 i = 0; i < (v4 & 7); i++) {
                (v3, v2) = step(v1, v2);
                if (v2 & 4 == 4) { return (v0 * 1) ^ (v3 * 4) ^ (v5 * 6) ^ (v7 * 8) ^ i; }
            }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8);
        }
    }

    // Two branch-and-loop pairs in a row, so the second join's predecessors sit
    // on both sides of it in layout order.
    function twoBranchLoops(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1; uint256 v1 = c + 2; uint256 v2 = b + 3;
            uint256 v3 = v1 - v2; uint256 v4 = v0 + v1; uint256 v5 = v1 ^ v2;
            uint256 v6 = v5 ^ v1; uint256 v7 = v3 + v6;
            if (v2 & 1 == 1) { v7 = v6 * v4; } else { v2 = mix(v6); }
            for (uint256 i = 0; i < (v4 & 3); i++) { (v3, v2) = step(v1, v2); }
            if (v3 & 1 == 1) { v5 = v7 * v2; } else { v6 = mix(v3); }
            for (uint256 i = 0; i < (v2 & 3); i++) { (v5, v7) = step(v6, v7); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8);
        }
    }

    // A `while (true)` loop whose only exit is a branch in its middle, followed
    // by a second loop that joins the exit.
    function whileTrueThenLoop(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1; uint256 v1 = c + 2; uint256 v2 = b + 3;
            uint256 v3 = v1 - v2; uint256 v4 = v0 + v1; uint256 v5 = v1 ^ v2;
            uint256 v6 = v5 ^ v1; uint256 v7 = v3 + v6; uint256 n = 0;
            if (v2 & 1 == 1) { v7 = v6 * v4; } else { v2 = mix(v6); }
            while (true) {
                (v3, v2) = step(v1, v2);
                n++;
                if (n > (v4 & 3)) break;
                v5 = v5 ^ n;
            }
            for (uint256 i = 0; i < (v2 & 3); i++) { (v6, v7) = step(v6, v7); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ n;
        }
    }
}
