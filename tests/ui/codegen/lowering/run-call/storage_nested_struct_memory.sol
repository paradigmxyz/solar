//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: copy((uint8[],uint8[2])[][]) [[([1, 2], [3, 7]), ([11, 13], [17, 19])]] => 35
//@ run-call: copyFixed((uint8[],uint8[2])[][1]) [[([1, 2], [3, 7]), ([11, 13], [17, 19])]] => 35
//@ run-call: copyDynamicFixed((uint8[],uint8[2])[1][]) [[([1, 2], [3, 7])], [([11, 13], [17, 19])]] => 35
// ported-from: test/libsolidity/semanticTests/array/copying/nested_array_of_structs_memory_to_storage.sol

pragma abicoder v2;

contract StorageNestedStructMemory {
    struct Entry {
        uint8[] values;
        uint8[2] pair;
    }

    Entry[][] private entries;
    Entry[][1] private fixedOuter;
    Entry[1][] private dynamicOuterFixed;

    function copy(Entry[][] memory input) external returns (uint256) {
        entries = input;
        return entries[0][1].values[1] + entries[0][0].pair[0] + entries[0][1].pair[1];
    }

    function copyFixed(Entry[][1] memory input) external returns (uint256) {
        fixedOuter = input;
        return fixedOuter[0][1].values[1] + fixedOuter[0][0].pair[0] + fixedOuter[0][1].pair[1];
    }

    function copyDynamicFixed(Entry[1][] memory input) external returns (uint256) {
        dynamicOuterFixed = input;
        return dynamicOuterFixed[1][0].values[1] + dynamicOuterFixed[0][0].pair[0]
            + dynamicOuterFixed[1][0].pair[1];
    }
}
