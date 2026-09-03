//@ codegen-matrix: standard
//@ run-call: writeStructs => 711
//@ run-call: sharedBytesDefault => true

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

    function sharedBytesDefault() external pure returns (bool result) {
        bytes[] memory values = new bytes[](2);
        assembly {
            result := eq(mload(add(values, 32)), mload(add(values, 64)))
        }
    }
}
