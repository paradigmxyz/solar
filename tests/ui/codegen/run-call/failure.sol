//@ run-call-fail: empty()
//@ run-call-fail: panic() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000001

contract RunCallFail {
    function empty() external pure {
        revert();
    }

    function panic() external pure {
        assert(false);
    }
}
