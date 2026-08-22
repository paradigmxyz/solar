//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: copy((uint256[])[]) [([1, 2, 3]), ([9])] => 14
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
}
