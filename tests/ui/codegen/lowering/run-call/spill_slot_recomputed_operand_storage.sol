//@ codegen-matrix: standard
//@ run-call: g 1, 2, 33 => 0x6ae
//@ run-call: g 0, 0, 0 => 0x1d3
//@ run-call: g 3, 5, 7 => 0x207260768a9ac672c3a
//@ run-call: g 7, 11, 13 => 0x44add4b6a561351c0ea

// The same spill-slot reuse as `spill_slot_recomputed_operand.sol`, with the
// rebuilt value crossing storage reads and writes as well as the loop: the
// branch arms store to `acc` and `s`, so the join reloads more values than the
// stack carries and the frame is under enough pressure to color the operand's
// slot together with a value the storage arm defines.
contract SpillSlotRecomputedOperandStorage {
    uint256 public acc;
    uint256[8] public s;

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

    function g(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        unchecked {
            uint256 v0 = a * 1 + b * 2 + 3;
            uint256 v1 = a * 2 + b * 3 + 4;
            uint256 v2 = a * 3 + b * 4 + 5;
            uint256 v4 = a * 5 + b * 6 + 7;
            uint256 v5 = a * 6 + b * 7 + 8;
            uint256 v8 = a * 9 + b * 10 + 11;
            uint256 v9 = a * 10 + b * 11 + 12;
            uint256 v12 = a * 13 + b * 14 + 15;
            uint256 v13 = a * 14 + b * 15 + 16;
            uint256 t0 = 0;
            while (true) {
                t0++;
                v5 ^= t0;
                if (t0 > (v9 & 3)) break;
                v13 = mix(v5);
            }
            (v9, v8) = step(v4, v12);
            v4 = s[v2 & 7] + acc + v12;
            if (v13 & 2 == 0) {
                acc += v5;
                s[v2 & 7] = v5;
            } else {
                v1 = mix(v4) ^ v2;
            }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v4 * 5) ^ (v5 * 6) ^ (v8 * 9) ^ (v9 * 10)
                ^ (v12 * 13) ^ (v13 * 14);
        }
    }
}
