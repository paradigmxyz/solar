//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: storeCalldata((uint8,uint16,bytes2,uint8)) (1, 255, 0x6162, 15) => 26612
//@[none, gas, size] run-call: storeMemory((uint8,uint16,bytes2,uint8)) (1, 255, 0x6162, 15) => 26612
//@[none, gas, size] run-call: storeSmall(uint16[]) [1, 2, 3] => 2
//@[none, gas, size] run-call: storeLeft(bytes2[]) [0x6162, 0x6364, 0x6566] => 99
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
