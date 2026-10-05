//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f => 234
//@ run-call: discardOptions => 23
//@ run-call: optionMembers => true
//@ run-call: directSelector => true
//@ run-call: pointerSelector true => true
//@ run-call: pointerSelector false => true
//@ run-call: returnedSelector => true

contract ExternalFunctionPointerOptions {
    uint256 marker;
    uint256 receiverEffects;

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

    function fail(uint256) external payable { revert(); }

    function receiver() internal returns (ExternalFunctionPointerOptions) {
        receiverEffects++;
        return this;
    }

    function functionValue() internal returns (function(uint256) external payable) {
        receiverEffects++;
        return this.fail;
    }

    function discardOptions() external returns (uint256) {
        address(this).call{gas: gasOpt(), value: valueOpt()};
        return marker;
    }

    function optionMembers() external returns (bool) {
        return this.sink{gas: gasOpt()}.address == address(this)
            && this.sink{gas: gasOpt()}.selector == bytes4(keccak256("sink(uint256)"))
            && marker == 22;
    }

    function directSelector() external returns (bool) {
        bytes4 selector = receiver().fail{value: 1 + valueOpt(), gas: gasOpt()}.selector;
        return selector == bytes4(keccak256("fail(uint256)")) && marker == 32 && receiverEffects == 1;
    }

    function pointerSelector(bool alternate) external returns (bool) {
        function(uint256) external payable pointer = alternate ? this.sink : this.fail;
        bytes4 selector = (pointer{gas: gasOpt(), value: 1 + valueOpt()}).selector;
        bytes4 expected = alternate ? bytes4(keccak256("sink(uint256)")) : bytes4(keccak256("fail(uint256)"));
        return selector == expected && marker == 23 && receiverEffects == 0;
    }

    function returnedSelector() external returns (bool) {
        bytes4 selector = functionValue(){gas: gasOpt(), value: 1 + valueOpt()}.selector;
        return selector == bytes4(keccak256("fail(uint256)")) && marker == 23 && receiverEffects == 1;
    }
}
