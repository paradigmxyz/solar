//@ codegen-matrix: standard
//@ run-call: writeStructs => 711

contract NewReferenceArrayDefaults {
    struct Item {
        uint256 value;
    }

    function writeStructs() external pure returns (uint256) {
        Item[] memory items = new Item[](2);
        items[0].value = 7;
        items[1].value = 11;
        return items[0].value * 100 + items[1].value;
    }
}
