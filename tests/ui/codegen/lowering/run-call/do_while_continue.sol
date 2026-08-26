//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: falseCondition() => 42
//@ run-call: countToThree() => 3
//@ run-call: conditionSideEffect() => 2, 2
//@ run-call: skipRemainder() => 20
//@ run-call: nested() => 6
//@ run-call: whileControl() => 3
//@ run-call: whileConditionalContinue() => 22
//@ run-call: exitUsesPreviousLoopValue() => 5
//@ run-call: once() => 1
//@ run-call: conditionSideEffects() => 22
//@ run-call: nestedSideEffects() => 24
//@ run-call: checkedDecrementLatch() => 10
// ported-from: test/libsolidity/semanticTests/statements/do_while_loop_continue.sol

contract DoWhileContinue {
    function falseCondition() external pure returns (uint256) {
        uint256 i;
        do {
            if (i > 0) return 0;
            ++i;
            continue;
        } while (false);
        return 42;
    }

    function countToThree() external pure returns (uint256 i) {
        do {
            ++i;
            continue;
        } while (i < 3);
    }

    function conditionSideEffect() external pure returns (uint256 i, uint256 checks) {
        do {
            ++i;
            continue;
        } while (++checks < 2);
    }

    function skipRemainder() external pure returns (uint256 sum) {
        uint256 i;
        do {
            ++i;
            if (i < 3) continue;
            sum += 10;
        } while (i < 4);
    }

    function nested() external pure returns (uint256 total) {
        uint256 outer;
        do {
            ++outer;
            uint256 inner;
            do {
                ++inner;
                ++total;
                continue;
            } while (inner < 3);
        } while (outer < 2);
    }

    function whileControl() external pure returns (uint256 i) {
        while (i < 3) {
            ++i;
            continue;
        }
    }

    function whileConditionalContinue() external pure returns (uint256 sum) {
        uint256 i;
        while (i < 4) {
            ++i;
            if (i < 3) {
                ++sum;
                continue;
            }
            sum += 10;
        }
    }

    function exitUsesPreviousLoopValue() external pure returns (uint256 x) {
        x = 1;
        uint256 i;
        do {
            ++i;
            continue;
            x += x;
        } while (i < 4);
        x += i;
    }

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

    function nestedSideEffects() external pure returns (uint256) {
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
