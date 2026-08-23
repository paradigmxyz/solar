//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f() => 1234

contract LowLevelCallEvaluationOrder {
    uint256 marker;

    function receiver() internal returns (address payable) {
        marker = marker * 10 + 1;
        return payable(address(this));
    }

    function gasOpt() internal returns (uint256) {
        marker = marker * 10 + 2;
        return gasleft();
    }

    function valueOpt() internal returns (uint256) {
        marker = marker * 10 + 3;
        return 0;
    }

    function data() internal returns (bytes memory) {
        marker = marker * 10 + 4;
        return "";
    }

    receive() external payable {}

    function f() external returns (uint256) {
        (bool ok,) = receiver().call{gas: gasOpt(), value: valueOpt()}(data());
        require(ok);
        return marker;
    }
}
