//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: storeCalldata (1, 255, 0x6162, 15) => 26612
//@ run-call: storeMemory (1, 255, 0x6162, 15) => 26612
//@ run-call: storeSmall [1, 2, 3] => 2
//@ run-call: storeLeft [0x6162, 0x6364, 0x6566] => 99
// ported-from: test/libsolidity/semanticTests/userDefinedValueType/calldata_to_storage.sol
// ported-from: test/libsolidity/semanticTests/userDefinedValueType/memory_to_storage.sol

pragma abicoder v2;

type Small is uint16;
type Left is bytes2;

contract UserDefinedStorageConversion {
    struct Entry {
        uint8 a;
        Small b;
        Left c;
        uint8 d;
    }

    Entry private entry;
    Small[] private small;
    Left[] private left;

    function storeCalldata(Entry calldata value) external returns (uint256) {
        entry = value;
        return uint256(entry.a) * 1000 + uint256(Small.unwrap(entry.b)) * 100
            + uint256(uint8(bytes1(Left.unwrap(entry.c)))) + entry.d;
    }

    function storeMemory(Entry memory value) external returns (uint256) {
        entry = value;
        return uint256(entry.a) * 1000 + uint256(Small.unwrap(entry.b)) * 100
            + uint256(uint8(bytes1(Left.unwrap(entry.c)))) + entry.d;
    }

    function storeSmall(Small[] calldata values) external returns (uint256) {
        small = values;
        return Small.unwrap(small[1]);
    }

    function storeLeft(Left[] calldata values) external returns (uint256) {
        left = values;
        return uint256(uint8(bytes1(Left.unwrap(left[1]))));
    }
}
