//@ codegen-matrix: standard
//@ run-call: sum [0, 0, 3, 3] => 70
//@ run-call: sum [3, 0] => 30
//@ run-call: write true, 9 => 5, 9
//@ run-call: write false, 9 => 9, 0

// A storage reference declared without a value points at slot zero, like in
// solc. Its declaration must bind that slot so later branches and loop
// iterations can read it or merge it with assigned references.
contract StorageReferenceUnassigned {
    struct Item {
        uint256 value;
    }

    uint256 slotZero = 5;
    mapping(uint256 => Item) items;

    constructor() {
        items[3].value = 30;
    }

    function sum(uint256[] calldata keys) external view returns (uint256 total) {
        uint256 last;
        Item storage item;
        for (uint256 i; i < keys.length; i++) {
            if (last != keys[i]) {
                last = keys[i];
                item = items[last];
            } else {
                item = item;
            }
            total += item.value;
        }
    }

    function write(bool assign, uint256 value) external returns (uint256, uint256) {
        Item storage item;
        if (assign) {
            item = items[1];
        } else {
            item = item;
        }
        item.value = value;
        return (slotZero, items[1].value);
    }
}
