//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f() => 234

contract ExternalFunctionPointerOptions {
    uint256 marker;

    function sink(uint256) external payable {}

    function gasOpt() internal returns (uint256) {
        marker = marker * 10 + 2;
        return gasleft();
    }

    function valueOpt() internal returns (uint256) {
        marker = marker * 10 + 3;
        return 0;
    }

    function arg() internal returns (uint256) {
        marker = marker * 10 + 4;
        return 7;
    }

    function f() external returns (uint256) {
        function(uint256) external payable fp = this.sink;
        fp{gas: gasOpt(), value: valueOpt()}(arg());
        return marker;
    }
}
