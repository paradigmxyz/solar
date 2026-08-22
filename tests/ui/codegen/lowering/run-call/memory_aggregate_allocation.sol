//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: allocate => 143
// ported-from: test/libsolidity/semanticTests/array/create_memory_array.sol

contract MemoryAggregateAllocation {
    struct Entry {
        uint256[2] fixedValues;
        bytes data;
    }

    function allocate() external pure returns (uint256) {
        bytes memory bytesValue = new bytes(35);
        bytesValue[34] = "A";

        uint256[2][] memory arrays = new uint256[2][](4);
        arrays[3][1] = 8;

        Entry[] memory entries = new Entry[](3);
        entries[2].fixedValues[1] = 4;
        entries[2].data = new bytes(6);
        entries[2].data[5] = "B";

        return uint8(bytesValue[34]) + arrays[3][1] + entries[2].fixedValues[1]
            + uint8(entries[2].data[5]);
    }
}
