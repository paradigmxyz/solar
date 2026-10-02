//@ codegen-matrix: standard
//@ run-call: lots 0, 1, 0 => 0, 0
//@ run-call: lots 999999, 1, 3 => 2, 0
//@ run-call: lots 5000999, 1, 3 => 3, 2
//@ run-call: lots 0, 400000, 5 => 2, 0
//@ run-call: lots 0, 1000000, 7 => 6, 1
//@ run-call: lots 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 1, 1 => 1, 1
//@ run-call-fail: lots 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 1, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// Counters that only some iterations advance cannot wrap within any affordable
// number of iterations, so their increments lose their overflow checks while
// the amount's own check stays.
contract ConditionalCounters {
    function lots(uint256 amount, uint256 step, uint256 count)
        external
        pure
        returns (uint256 filled, uint256 capped)
    {
        for (uint256 i = 0; i < count; ++i) {
            if (amount / 1e6 != 0) ++filled;
            if (amount / 1e3 > 5000) ++capped;
            amount += step;
        }
    }
}
