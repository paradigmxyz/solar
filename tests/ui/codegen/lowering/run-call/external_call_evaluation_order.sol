//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: externalReceiver() => 14
//@ run-call: callOptions() => 234

contract ExternalCallEvaluationOrder {
    uint256 marker;

    function receiver() internal returns (ExternalCallEvaluationOrder) {
        marker = marker * 10 + 1;
        return this;
    }

    function arg() internal returns (uint256) {
        marker = marker * 10 + 4;
        return 7;
    }

    function gasOpt() internal returns (uint256) {
        marker = marker * 10 + 2;
        return gasleft();
    }

    function valueOpt() internal returns (uint256) {
        marker = marker * 10 + 3;
        return 0;
    }

    function sink(uint256) external payable {}

    function externalReceiver() external returns (uint256) {
        receiver().sink(arg());
        return marker;
    }

    function callOptions() external returns (uint256) {
        this.sink{gas: gasOpt(), value: valueOpt()}(arg());
        return marker;
    }
}
