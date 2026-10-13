//@ codegen-matrix: standard
//@ run-call: count => 100

// An `int24` counter from -50 wraps from negative to positive within 2^24
// steps, so the loop exit `tick < 50` must stay.
contract NarrowSignedCounter {
    function count() external pure returns (uint256 n) {
        unchecked {
            for (int24 tick = -50; tick < 50;) {
                id(tick++);
                n++;
                if (n > 1000) return 9999;
            }
        }
    }

    function id(int24 tick) internal pure returns (int24) {
        return tick;
    }
}
