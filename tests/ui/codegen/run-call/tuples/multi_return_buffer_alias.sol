//@ compile-flags: -Ogas
//@ run-call: run 3, 9 => 59, 53

// Tail results of a multi-result internal call travel through a caller-side
// buffer written only by backend lowering. Memory analysis must treat the
// call as a memory writer, or the second call's buffer loads could be reused
// from the first call.
contract MultiReturnBufferAlias {
    function run(uint256 a, uint256 b) external pure returns (uint256, uint256) {
        (uint256 r1, uint256 t1) = pair(a);
        (uint256 r2, uint256 t2) = pair(b);
        unchecked {
            return (r1 + t2, t1 + r2);
        }
    }

    // The loop keeps this memory-clean helper outlined.
    function pair(uint256 x) internal pure returns (uint256, uint256) {
        unchecked {
            uint256 r = x;
            for (uint256 i = 0; i < x % 3 + 2; i++) {
                r = r * 2 + i;
            }
            return (r, r + x);
        }
    }
}
