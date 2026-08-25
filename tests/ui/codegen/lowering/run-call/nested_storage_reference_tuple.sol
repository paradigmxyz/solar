//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: fromCall() => 10, 31, 41, 7
//@ run-call: fromTuple() => 10, 31, 41, 7

contract NestedStorageReferenceTuple {
    struct Item {
        uint256 value;
    }

    Item[3] private items;

    function fromCall() external returns (uint256, uint256, uint256, uint256) {
        initialize();
        Item storage first = items[0];
        Item storage second = items[1];
        uint256 marker;
        ((first, second), marker) = (pair(), 7);
        first.value = 31;
        second.value = 41;
        return (items[0].value, items[1].value, items[2].value, marker);
    }

    function fromTuple() external returns (uint256, uint256, uint256, uint256) {
        initialize();
        Item storage first = items[0];
        Item storage second = items[1];
        uint256 marker;
        ((first, second), marker) = ((items[1], items[2]), 7);
        first.value = 31;
        second.value = 41;
        return (items[0].value, items[1].value, items[2].value, marker);
    }

    function initialize() private {
        items[0].value = 10;
        items[1].value = 20;
    }

    function pair() private view returns (Item storage first, Item storage second) {
        first = items[1];
        second = items[2];
    }
}
