//@ run-call: compound() => 1, 17, 20
//@ run-call: postIncrement() => 1, 11, 20

contract LValueEvaluationOrder {
    uint256[] values;
    uint256 index;

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
}
