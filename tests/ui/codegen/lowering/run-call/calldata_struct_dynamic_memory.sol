//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: copy [([1, 2, 3]), ([9])] => 14
//@ run-call: tupleCopy [11, 22, 33] => 22, 11
// ported-from: test/libsolidity/semanticTests/array/copying/array_of_structs_containing_arrays_calldata_to_memory.sol

pragma abicoder v2;

contract CalldataStructDynamicMemory {
    struct Entry {
        uint256[] values;
    }

    function copy(Entry[] calldata input) external pure returns (uint256) {
        Entry[] memory values = input;
        return values.length + values[0].values.length + values[1].values[0];
    }

    function tupleCopy(uint256[] calldata input) external pure returns (uint256, uint256) {
        Entry memory entry;
        (input, entry.values) = split(input);
        return (input[0], entry.values[0]);
    }

    function split(uint256[] calldata input) internal pure returns (uint256[] calldata, uint256[] calldata) {
        return (input[1:], input);
    }
}
