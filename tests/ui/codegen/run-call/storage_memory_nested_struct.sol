//@ run-call: copy => 71
// ported-from: test/libsolidity/semanticTests/array/copying/storage_memory_nested_struct.sol

contract StorageMemoryNestedStruct {
    struct Entry {
        uint8 first;
        uint8 second;
        uint256[] values;
    }

    Entry[3][] entries;

    function copy() external returns (uint256) {
        entries.push();
        entries.push();
        entries[0][1].first = 11;
        entries[0][1].second = 12;
        entries[0][1].values.push(1);
        entries[0][1].values.push(2);
        entries[0][1].values.push(3);
        entries[1][2].first = 21;
        entries[1][2].second = 22;
        entries[1][2].values.push(4);
        entries[1][2].values.push(5);
        entries[1][2].values.push(6);

        Entry[3][] memory copied = entries;
        return copied[0][1].first + copied[0][1].second + copied[0][1].values[0]
            + copied[1][2].first + copied[1][2].second + copied[1][2].values[0];
    }
}
