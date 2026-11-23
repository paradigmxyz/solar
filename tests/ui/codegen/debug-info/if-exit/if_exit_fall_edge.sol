//@ revisions: none gas
//@[none] compile-flags: -Onone
//@[gas] compile-flags: -Ogas
//@ compile-flags: --emit=srcmap-runtime -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime-scheduled
//@[none] filecheck: --check-prefix=NONE
//@[gas] filecheck: --check-prefix=GAS
//@ normalize-stdout-test: "(?s).+" -> ""

// An arm that only assigns a local leaves no code of its own: `cfg-simplify` bypasses it and the
// edge from the branch to the join holds the assignment. `-Onone` keeps the arm's block, which
// names the assignment, and the push of the assignment keeps its origin. The block layout may put that edge's code
// inline after the branch instead of in a trampoline. Either way the jump that leaves the arm has
// no location, as it has in the arm's own block, and the origin of the condition does not come
// back after the arm ran.
contract C {
    // NONE-LABEL: ; function states(
    // NONE: push 33 !metadata(span=
    // NONE-NEXT: swap 2{{$}}
    // NONE-NEXT: pop{{$}}
    // NONE-NEXT: jump {{bb[0-9]+}}{{$}}
    function states(uint256 n) external pure returns (uint256) {
        uint256 state = 0;
        for (uint256 i = 0; i < n; i++) {
            if (state == 0) {
                if (i == 3) {
                    state = 11;
                }
            } else if (state == 11) {
                if (i == 5) {
                    state = 22;
                }
            } else if (state == 22) {
                state = 33;
            }
        }
        return state;
    }

    // GAS-LABEL: ; function bisect(
    // GAS: [loop]:{{$}}
    // GAS: jumpi !metadata(span=
    // GAS-NEXT: swap 1 !metadata(span=
    // GAS-NEXT: pop !metadata(span=
    // GAS-NEXT: jump {{bb[0-9]+}}{{$}}
    function bisect(uint256 n, uint256 x) external pure returns (uint256 lo) {
        uint256 hi = n;
        while (lo < hi) {
            uint256 mid = (lo + hi) >> 1;
            if (mid * mid < x) lo = mid + 1;
            else hi = mid;
        }
    }
}
