//@ codegen-matrix: standard
//@ run-call: loopTuple 0, 1, 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffe326ec342f968e7d18
//@ run-call: loopTuple 1, 0, 2 => 0xf9
//@ run-call: loopTuple 3, 5, 7 => 0xe6c89e5e834b8c720
//@ run-call: loopTuple 0, 0, 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffa1
//@ run-call: loopTuple 2, 3, 1 => 0xfffffffffffffffffffffffffffffffffffffffffffffff8c9bb0d0be5a39f0c
//@ run-call: loopTupleInline 0, 1, 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffe326ec342f968e7d18
//@ run-call: loopTupleInline 1, 0, 2 => 0xf9
//@ run-call: loopTupleInline 3, 5, 7 => 0xe6c89e5e834b8c720
//@ run-call: loopTupleInline 0, 0, 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffa1
//@ run-call: loopTupleInline 2, 3, 1 => 0xfffffffffffffffffffffffffffffffffffffffffffffff8c9bb0d0be5a39f0c

// A branch whose arms join a runtime-bounded loop, with more live values than
// the loop header carries on the stack. One arm redefines a value the other
// keeps, so the join block owns the frame store for every value it drops from
// the stack, whichever arm reached it.
contract LoopTupleSpillAcrossBranch {
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

    function loopTuple(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = v1 - v2;
            uint256 v4 = v0 + v1;
            uint256 v5 = v1 - v2;
            uint256 v6 = v5 ^ v1;
            uint256 v7 = v3 + v6;
            if (v2 & 1 == 1) {
                v7 = v6 * v4;
            } else {
                v2 = mix(v6);
            }
            for (uint256 i = 0; i < (v4 & 3); i++) {
                (v3, v2) = step(v1, v2);
            }
            return (v0 * 1) ^
                (v1 * 2) ^
                (v2 * 3) ^
                (v3 * 4) ^
                (v4 * 5) ^
                (v5 * 6) ^
                (v6 * 7) ^
                (v7 * 8);
        }
    }

    function loopTupleInline(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1;
            uint256 v1 = c + 2;
            uint256 v2 = b + 3;
            uint256 v3 = v1 - v2;
            uint256 v4 = v0 + v1;
            uint256 v5 = v1 - v2;
            uint256 v6 = v5 ^ v1;
            uint256 v7 = v3 + v6;
            if (v2 & 1 == 1) {
                v7 = v6 * v4;
            } else {
                v2 = mix(v6);
            }
            for (uint256 i = 0; i < (v4 & 3); i++) {
                (v3, v2) = (v1 * 3 + v2, v1 ^ (v2 << 1));
            }
            return (v0 * 1) ^
                (v1 * 2) ^
                (v2 * 3) ^
                (v3 * 4) ^
                (v4 * 5) ^
                (v5 * 6) ^
                (v6 * 7) ^
                (v7 * 8);
        }
    }
}
