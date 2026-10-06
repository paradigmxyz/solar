//@ codegen-matrix: standard
//@ run-call: storageTuple 0x010203, 0xaabb => 0x010203, 0xaabb
//@ run-call: storageTuple 0x, 0x => 0x, 0x
//@ run-call: memoryTuple [11, 22, 33] => 22, 11

contract CalldataTupleMaterialization {
    struct Entry {
        uint256[] values;
    }

    bytes internal first;
    bytes internal second;

    function storageTuple(bytes calldata a, bytes calldata b) external returns (bytes memory, bytes memory) {
        (first, second) = (a, b);
        return (first, second);
    }

    function memoryTuple(uint256[] calldata input) external pure returns (uint256, uint256) {
        Entry memory entry;
        (input, entry.values) = split(input);
        return (input[0], entry.values[0]);
    }

    function split(uint256[] calldata input) internal pure returns (uint256[] calldata, uint256[] calldata) {
        return (input[1:], input);
    }
}
