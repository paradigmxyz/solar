// The ternary's incoming sums, `x + 4 + 32` and `x + 36`, share one key, so cse sinks them
// into one sum after the join.
contract PhiSink {
    function sum(uint256 x, bool c) external pure returns (uint256) {
        uint256 y = c ? x + 4 + 32 : x + 36;
        return y * 3;
    }
}
