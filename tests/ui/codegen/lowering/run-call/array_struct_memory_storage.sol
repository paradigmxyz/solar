//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: copy => 10
// ported-from: test/libsolidity/semanticTests/array/copying/array_of_structs_containing_arrays_memory_to_storage.sol

contract ArrayStructMemoryStorage {
    struct Entry {
        uint136 id;
        uint128[3] fixedValues;
        uint128[] dynamicValues;
    }

    Entry[] entries;

    function copy() external returns (uint256) {
        Entry[] memory values = new Entry[](3);
        values[1] = Entry(0, [uint128(1), 2, 3], new uint128[](3));
        values[1].dynamicValues[0] = 1;
        values[1].dynamicValues[1] = 2;
        values[1].dynamicValues[2] = 3;
        entries = values;
        return entries[1].fixedValues.length + entries[1].dynamicValues.length
            + entries[1].fixedValues[2] + entries[1].dynamicValues[0];
    }
}
