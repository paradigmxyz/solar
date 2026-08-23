//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: compound() => 1, 17, 20
//@ run-call: postIncrement() => 1, 11, 20
//@ run-call: pushSideEffect() => 2, 9, 7

contract LValueEvaluationOrder {
    uint256[] values;
    uint256 index;
    uint256[] pushed;

    constructor() {
        values.push(10);
        values.push(20);
    }

    function compound() external returns (uint256, uint256, uint256) {
        values[index++] += 7;
        return (index, values[0], values[1]);
    }

    function postIncrement() external returns (uint256, uint256, uint256) {
        values[index++]++;
        return (index, values[0], values[1]);
    }

    function value() external returns (uint256) {
        pushed.push(9);
        return 7;
    }

    function pushSideEffect() external returns (uint256, uint256, uint256) {
        pushed.push(this.value());
        return (pushed.length, pushed[0], pushed[1]);
    }
}
