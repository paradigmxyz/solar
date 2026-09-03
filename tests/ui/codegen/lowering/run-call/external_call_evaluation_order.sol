//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: externalReceiver => 14
//@ run-call: callOptions => 234
//@ run-call: dirtyGasOption => false
//@ run-call: dirtySend; value=1 => true
//@ run-call: dirtyIndex => 7
//@ run-call: staticReceiverMutability => 1

contract MutatingBase {
    uint256 value;

    function mutate() external virtual returns (uint256) {
        return ++value;
    }
}

contract ExternalCallEvaluationOrder is MutatingBase {
    uint256 marker;
    MutatingBase private immutable target = new MutatingBase();

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

    function dirtyUint8(uint256 raw) internal pure returns (uint8 value) {
        assembly {
            value := raw
        }
    }

    function dirtyGasOption() external returns (bool success) {
        (success, ) = address(this).call{gas: dirtyUint8(0x10000)}(
            abi.encodeCall(this.sink, (0))
        );
    }

    function dirtySend() external payable returns (bool) {
        return payable(address(0xbeef)).send(dirtyUint8(0x100));
    }

    function dirtyIndex() external pure returns (uint256) {
        uint256[257] memory values;
        values[0] = 7;
        values[256] = 9;
        return values[dirtyUint8(0x100)];
    }

    function mutate() external view override returns (uint256) {
        return value + 7;
    }

    function staticReceiverMutability() external returns (uint256) {
        return target.mutate();
    }
}
