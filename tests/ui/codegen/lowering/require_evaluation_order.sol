//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: RequireEvaluationOrder::customError() => 1
//@ run-call: RequireEvaluationOrder::stringError() => 1
//@ run-call: RequireEvaluationOrder::earlyReturn() => 7
//@ run-call-fail: RequireEvaluationOrder::customFailure() => 0x002ff0670000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: RequireEvaluationOrder::stringFailure() => 0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000017800000000000000000000000000000000000000000000000000000000000000

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
