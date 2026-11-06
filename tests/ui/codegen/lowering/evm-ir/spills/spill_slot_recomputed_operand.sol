//@ codegen-matrix: standard
//@ run-call: g 1, 2, 33 => 0x8a708a824f612c92ee
//@ run-call: g 3, 5, 7 => 0x12ed62efd0da4917f7a

// `-O gas` colors the spill frame, letting two values whose live ranges do not
// overlap share one slot. A value the backend can rebuild instead of storing
// reads its operands where the rebuild happens, which is past the point
// liveness drops them, so the operand's slot has to stay intact until the last
// rebuild and not only until its own last use.
//
// Here `v2 * 3 + v3` is rebuilt at the join after the loops, from operands the
// branch arm defined before the tuple call, while the arm's own `mload` result
// takes the slot one of those operands still needs.
contract SpillSlotRecomputedOperand {
    function mix(uint256 x) internal pure returns (uint256) {
        unchecked {
            return x * 0x9e3779b97f4a7c15 + 1;
        }
    }

    function step(uint256 x, uint256 y) internal pure returns (uint256, uint256) {
        unchecked {
            return (x * 3 + y, x ^ (y << 1));
        }
    }

    function g(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a * 1 + b * 2 + 3;
            uint256 v1 = a * 2 + b * 3 + 4;
            uint256 v2 = a * 3 + b * 4 + 5;
            uint256 v3 = a * 4 + b * 5 + 6;
            uint256 v4 = a * 5 + b * 6 + 7;
            uint256 v5 = a * 6 + b * 7 + 8;
            uint256 v6 = a * 7 + b * 8 + 9;
            uint256 v7 = a * 8 + b * 9 + 10;
            uint256 v8 = a * 9 + b * 10 + 11;
            v6 = mix(v6) ^ v3;
            if (v6 & 4 == 0) {
                for (uint256 q1 = 0; q1 < (v6 & 3); q1++) {}
            } else {
                (v3, v2) = step(v3, v7);
                for (uint256 q1 = 0; q1 < (v7 & 3); q1++) {}
                uint256 t1 = 0;
                while (true) {
                    t1++;
                    v0 ^= t1;
                    if (t1 > (v4 & 3)) break;
                    v6 = mix(v0);
                }
            }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7)
                ^ (v7 * 8) ^ (v8 * 9);
        }
    }
}
