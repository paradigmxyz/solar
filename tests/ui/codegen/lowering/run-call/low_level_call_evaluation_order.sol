//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f => 1234
//@ run-call: captureAcrossCalls => 0x010203, 0xab
//@ run-call: staticRead => true, 0x0000000000000000000000000000000000000000000000000000000000000007
//@ run-call: delegateWrite => 9
//@ run-call: preserveInput => 0x010203, 0x990203
//@ run-call: failedCapture => false, 0xdeadbeef

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

    function echo(bytes memory input) external pure returns (bytes memory) {
        return input;
    }

    function number() external pure returns (uint256) {
        return 7;
    }

    function write() external {
        marker = 9;
    }

    function fail() external pure {
        assembly {
            mstore(0, shl(224, 0xdeadbeef))
            revert(0, 4)
        }
    }

    function captureAcrossCalls() external returns (bytes memory, bytes memory) {
        (bool first, bytes memory a) = address(this).call(abi.encodeCall(this.echo, (hex"010203")));
        (bool second, bytes memory b) = address(this).call(abi.encodeCall(this.echo, (hex"ab")));
        require(first && second);
        return (abi.decode(a, (bytes)), abi.decode(b, (bytes)));
    }

    function staticRead() external view returns (bool, bytes memory) {
        return address(this).staticcall(abi.encodeCall(this.number, ()));
    }

    function delegateWrite() external returns (uint256) {
        marker = 1;
        (bool ok,) = address(this).delegatecall(abi.encodeCall(this.write, ()));
        require(ok);
        return marker;
    }

    function preserveInput() external returns (bytes memory, bytes memory) {
        bytes memory input = hex"010203";
        (bool ok, bytes memory result) = address(this).call(abi.encodeCall(this.echo, (input)));
        require(ok);
        input[0] = 0x99;
        return (abi.decode(result, (bytes)), input);
    }

    function failedCapture() external returns (bool, bytes memory) {
        return address(this).call(abi.encodeCall(this.fail, ()));
    }
}
