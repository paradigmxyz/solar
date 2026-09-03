// Tuple assignment where a later lvalue's index expression mutates the storage
// value being copied into an earlier memory lvalue.
// solc returns (99, 99): the storage-to-memory copy happens after all lvalues
// are evaluated. solar returns (10, 99): it copies eagerly.
// Source: tests/ui/codegen/lowering/run-call/storage_tuple_aliasing.sol
contract StorageToMemoryTupleOrder {
    struct Item {
        uint256 value;
    }

    Item[3] private items;

    function mutateSource() internal returns (uint256) {
        items[0].value = 99;
        return 0;
    }

    function memorySnapshot() external returns (uint256, uint256) {
        items[0].value = 10;
        Item memory copy;
        uint256[1] memory targets;
        (copy, targets[mutateSource()]) = (items[0], 1);
        return (copy.value, items[0].value);
    }
}
