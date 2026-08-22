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
//@[none] run-call: compound() => 1, 17, 20
//@[gas] run-call: compound() => 1, 17, 20
//@[size] run-call: compound() => 1, 17, 20
//@[none] run-call: postIncrement() => 1, 11, 20
//@[gas] run-call: postIncrement() => 1, 11, 20
//@[size] run-call: postIncrement() => 1, 11, 20
//@[none] run-call: pushSideEffect() => 2, 9, 7
//@[gas] run-call: pushSideEffect() => 2, 9, 7
//@[size] run-call: pushSideEffect() => 2, 9, 7

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
