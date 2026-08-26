//@ run-call: DoWhileContinue::once() => 1
//@ run-call: DoWhileContinue::conditionSideEffects() => 22
//@ run-call: DoWhileContinue::nested() => 24
//@ run-call: DoWhileContinue::checkedDecrementLatch() => 10

contract DoWhileContinue {
    function once() external pure returns (uint256 bodyRuns) {
        do {
            bodyRuns++;
            continue;
        } while (false);
    }

    function conditionSideEffects() external pure returns (uint256) {
        uint256 bodyRuns;
        uint256 checks;
        do {
            bodyRuns++;
            continue;
        } while (++checks < 2);
        return bodyRuns * 10 + checks;
    }

    function nested() external pure returns (uint256) {
        uint256 outer;
        uint256 innerRuns;
        do {
            outer++;
            uint256 checks;
            do {
                innerRuns++;
                continue;
            } while (++checks < 2);
            continue;
        } while (outer < 2);
        return outer * 10 + innerRuns;
    }

    function checkedDecrementLatch() external pure returns (uint256 sum) {
        for (uint256 q = 4; q != 0; --q) {
            sum += q;
        }
    }
}
