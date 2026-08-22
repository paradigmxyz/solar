//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: copy((uint256[])[]) [([1, 2, 3]), ([9])] => 14
// ported-from: test/libsolidity/semanticTests/array/copying/array_of_structs_containing_arrays_calldata_to_storage.sol

pragma abicoder v2;

contract StorageStructDynamicWordsCalldata {
    struct Entry {
        uint256[] values;
    }

    Entry[] private entries;

    function copy(Entry[] calldata input) external returns (uint256) {
        entries = input;
        return entries.length + entries[0].values.length + entries[1].values[0];
    }
}
