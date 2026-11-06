//@ codegen-matrix: standard
//@ run-call: storageTuple 0x010203, 0xaabb => 0x010203, 0xaabb
//@ run-call: storageTuple 0x, 0x => 0x, 0x
//@ run-call: memoryTuple [11, 22, 33] => 22, 11
//@ run-call: widenedTuple [5, 6] => 5, 6, 0
//@ run-call: sliceTuple [9, 0, 7, 0] => 0, 7, 0

contract CalldataTupleMaterialization {
    struct Entry {
        uint256[] values;
    }

    bytes internal first;
    bytes internal second;
    uint256[3] internal fixedValues;
    uint256[] internal dynamicValues;

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

    function widenedTuple(uint256[2] calldata input) external returns (uint256, uint256, uint256) {
        fixedValues[2] = 99;
        (fixedValues,) = pair(input);
        return (fixedValues[0], fixedValues[1], fixedValues[2]);
    }

    function pair(uint256[2] calldata input) internal pure returns (uint256[2] calldata, uint256) {
        return (input, 0);
    }

    function sliceTuple(uint256[] calldata input) external returns (uint256, uint256, uint256) {
        (dynamicValues,) = (input[1:], 0);
        return (dynamicValues[0], dynamicValues[1], dynamicValues[2]);
    }
}
