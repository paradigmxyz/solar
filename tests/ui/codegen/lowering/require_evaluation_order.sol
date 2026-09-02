//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: RequireEvaluationOrder::customError => 1
//@ run-call: RequireEvaluationOrder::stringError => 1
//@ run-call: RequireEvaluationOrder::earlyReturn => 7
//@ run-call-fail: RequireEvaluationOrder::customFailure => E(uint256)(1)
//@ run-call-fail: RequireEvaluationOrder::stringFailure => Error("x")

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
