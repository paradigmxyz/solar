//@ codegen-matrix: standard
//@ run-call: latchRevert 1000, 5 => 10
//@ run-call-fail: latchRevert 5, 10 => 0xa16662a700000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: latchRevert 2, 3 => 0xa16662a700000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000002

contract RevertPhiJoin {
    error W(uint256 a, uint256 b);

    // The latch branches to the loop header or to a revert whose payload words
    // are phis. The revert reads no other value, so the stack planner may enter
    // it with any words, but the latch must still copy the phi inputs.
    function latchRevert(uint256 a, uint256 n) external pure returns (uint256 s) {
        uint256 i = 0;
        uint256 x;
        uint256 y;
        while (true) {
            unchecked {
                s += i;
                ++i;
            }
            if (i < n) {
                if (s <= a) continue;
                x = s;
                y = 1;
            } else {
                if (s <= a) return s;
                x = i;
                y = 2;
            }
            revert W(x, y);
        }
    }
}
