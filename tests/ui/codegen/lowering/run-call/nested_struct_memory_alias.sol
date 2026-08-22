//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: check => 14
// ported-from: test/libsolidity/semanticTests/array/copying/nested_array_of_structs_memory_to_memory.sol

contract NestedStructMemoryAlias {
    struct Entry {
        uint8[] values;
        uint8[2] pair;
    }

    function check() external pure returns (uint256) {
        Entry[][] memory original = new Entry[][](1);
        original[0] = new Entry[](1);
        original[0][0].values = new uint8[](2);
        original[0][0].values[0] = 3;
        original[0][0].values[1] = 5;
        original[0][0].pair[0] = 7;
        original[0][0].pair[1] = 11;

        Entry[][] memory aliasValue = original;
        aliasValue[0][0].values[1] = 7;
        aliasValue[0][0].pair[0] = 0;
        return original[0][0].values[1] + aliasValue[0][0].values[1];
    }
}
