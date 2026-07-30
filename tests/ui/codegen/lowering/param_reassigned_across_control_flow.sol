//@compile-flags: -Zcodegen --emit=bin-runtime

// A parameter reassigned under control flow must merge through its frame slot
// like a reassigned local: a plain SSA binding only updates within a block, so
// a sibling arm or the next loop iteration would read a definition that cannot
// reach it. The slot address spans the parameter and return areas, so the
// store is staged until every parameter and return is registered — computing
// it mid-loop resolved to a different slot than the body's reads, which read
// back the branch value on the path that never took the branch.
//
// Verified against solc across both arms, nested arms, loop trip counts,
// early exit, several parameters, a memory parameter and a named return.
contract ParamReassignedAcrossControlFlow {
    function ifOnly(uint256 x, bool c) external pure returns (uint256) {
        if (c) {
            x = 7;
        }
        return x;
    }

    function nested(uint256 x, bool a, bool b) external pure returns (uint256) {
        if (a) {
            if (b) {
                x = 1;
            } else {
                x = 2;
            }
        } else if (b) {
            x = 3;
        }
        return x;
    }

    function loop(uint256 x, uint256 n) external pure returns (uint256) {
        for (uint256 i = 0; i < n; i++) {
            x = x + 2;
        }
        return x;
    }

    function memParam(bytes memory d, bool c) external pure returns (uint256) {
        if (c) {
            d = hex"aabbcc";
        }
        return d.length;
    }
}
