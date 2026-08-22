//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: falseCondition() => 42
//@[none, gas, size] run-call: countToThree() => 3
//@[none, gas, size] run-call: conditionSideEffect() => 2, 2
//@[none, gas, size] run-call: skipRemainder() => 20
//@[none, gas, size] run-call: nested() => 6
//@[none, gas, size] run-call: whileControl() => 3
//@[none, gas, size] run-call: whileConditionalContinue() => 22
//@[none, gas, size] run-call: exitUsesPreviousLoopValue() => 5
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
}
