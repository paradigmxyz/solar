//@ compile-flags: -Ogas
//@ run-call: f 0 => 0, 1
//@ run-call: f 3 => 0, 4
//@ run-call: f 6 => 6, 7

// Gas only: `loop-split` runs only at `-Ogas`.
// The loop header runs `visits += 1`, so the original loop must not repeat it
// for an iteration a split main loop declined.
contract LoopSplitHeaderEffects {
    uint256 public visits;

    function f(uint256 n) external returns (uint256 total, uint256 seen) {
        unchecked {
            for (uint256 i;; ++i) {
                visits += 1;
                if (i >= n) break;
                if (i + 2 < n) total += i;
            }
        }
        seen = visits;
    }
}
