//@ codegen-matrix: standard
//@ run-call: decodeCall 5 => 6
//@ run-call: decodeReturn 0x1234 => 1
//@ run-call: decodeCalldata 0x1234 => 2
//@ run-call: decodeCalldataLength 57896044618658097711785492504343953926634992332820282019728792003956564819967 => 3
//@ run-call-fail: decodeCalldataLength 57896044618658097711785492504343953926634992332820282019728792003956564819968
//@ run-call: decodeMemoryLength 57896044618658097711785492504343953926634992332820282019728792003956564819967 => 4
//@ run-call-fail: decodeMemoryLength 57896044618658097711785492504343953926634992332820282019728792003956564819968

// `abi.decode(data, ())` decodes nothing, but its data argument still runs and a length with the
// sign bit set reverts.
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

    function decodeCalldataLength(uint256 length) external pure returns (uint256) {
        bytes calldata data = msg.data[0:0];
        assembly {
            data.length := length
        }
        abi.decode(data, ());
        return 3;
    }

    function decodeMemoryLength(uint256 length) external pure returns (uint256) {
        bytes memory data = new bytes(0);
        assembly {
            mstore(data, length)
        }
        abi.decode(data, ());
        return 4;
    }
}
