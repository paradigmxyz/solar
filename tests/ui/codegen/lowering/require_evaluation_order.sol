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
//@[none] run-call: RequireEvaluationOrder::customError() => 1
//@[gas] run-call: RequireEvaluationOrder::customError() => 1
//@[size] run-call: RequireEvaluationOrder::customError() => 1
//@[none] run-call: RequireEvaluationOrder::stringError() => 1
//@[gas] run-call: RequireEvaluationOrder::stringError() => 1
//@[size] run-call: RequireEvaluationOrder::stringError() => 1
//@[none] run-call: RequireEvaluationOrder::earlyReturn() => 7
//@[gas] run-call: RequireEvaluationOrder::earlyReturn() => 7
//@[size] run-call: RequireEvaluationOrder::earlyReturn() => 7
//@[none] run-call-fail: RequireEvaluationOrder::customFailure() => 0x002ff0670000000000000000000000000000000000000000000000000000000000000001
//@[gas] run-call-fail: RequireEvaluationOrder::customFailure() => 0x002ff0670000000000000000000000000000000000000000000000000000000000000001
//@[size] run-call-fail: RequireEvaluationOrder::customFailure() => 0x002ff0670000000000000000000000000000000000000000000000000000000000000001
//@[none] run-call-fail: RequireEvaluationOrder::stringFailure() => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000017800000000000000000000000000000000000000000000000000000000000000
//@[gas] run-call-fail: RequireEvaluationOrder::stringFailure() => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000017800000000000000000000000000000000000000000000000000000000000000
//@[size] run-call-fail: RequireEvaluationOrder::stringFailure() => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000017800000000000000000000000000000000000000000000000000000000000000

contract RequireEvaluationOrder {
    error E(uint256);

    uint256 private counter;

    function customError() external returns (uint256) {
        counter = 0;
        require(true, E(increment()));
        return counter;
    }

    function stringError() external returns (uint256) {
        counter = 0;
        require(true, reason());
        return counter;
    }

    function earlyReturn() external pure returns (uint256) {
        require(true, E(returnSeven()));
        return 42;
    }

    function customFailure() external {
        counter = 0;
        require(false, E(increment()));
    }

    function stringFailure() external {
        counter = 0;
        require(false, reason());
    }

    function increment() internal returns (uint256) {
        return ++counter;
    }

    function reason() internal returns (string memory) {
        ++counter;
        return "x";
    }

    function returnSeven() internal pure returns (uint256) {
        assembly {
            mstore(0, 7)
            return(0, 32)
        }
    }
}
