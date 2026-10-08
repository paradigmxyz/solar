//@ codegen-matrix: standard
//@ run-call: decodeCall 5 => 6
//@ run-call: decodeReturn 0x1234 => 1
//@ run-call: decodeCalldata 0x1234 => 2

// `abi.decode(data, ())` decodes nothing, but its data argument still runs.
contract AbiDecodeEmptyTuple {
    uint256 calls;

    function record(uint256 value) internal returns (bytes memory) {
        calls += value;
        return abi.encode(value);
    }

    function decodeCall(uint256 value) external returns (uint256) {
        calls = 1;
        abi.decode(record(value), ());
        return calls;
    }

    function decodeVoid(bytes memory data) internal {
        calls += 1;
        return abi.decode(data, ());
    }

    function decodeReturn(bytes memory data) external returns (uint256) {
        decodeVoid(data);
        return calls;
    }

    function decodeCalldata(bytes calldata data) external returns (uint256) {
        calls = 2;
        abi.decode(data, ());
        return calls;
    }
}
