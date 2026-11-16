//@ run-call-fail: empty
//@ run-call-fail: panic => Panic(1)

contract RunCallFail {
    function empty() external pure {
        revert();
    }

    function panic() external pure {
        assert(false);
    }
}
