//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: bindAfterDeclaration 7, 11 => 11, 12
//@ run-call: rebind true, 3, 5, 17 => 0, 17
//@ run-call: rebind false, 3, 5, 17 => 17, 0
//@ run-call: rebindParameter 3, 5, 19 => 0, 19
//@ run-call: swap 3, 5, 23, 29 => 29, 23
//@ run-call: packedRebind false, 3, 5, 17 => 17, 0
//@ run-call: packedRebind true, 3, 5, 17 => 0, 17
//@ run-call: packedYulRebind false, 3, 5, 17 => 17, 0, 0
//@ run-call: packedYulRebind true, 3, 5, 17 => 17, 0, 0
//@ run-call: yulPackedOffset 17 => 1
//@ run-call: assignmentExpression 3, 5, 17 => 0, 17
//@ run-call: mappingAssignmentExpression 3, 5, 17 => 1, 0, 0, 17

contract StorageReferenceReassignment {
    struct Item {
        uint256 a;
        uint256 b;
    }

    mapping(uint256 => Item) items;

    struct PackedItem {
        uint256 whole;
        uint8 first;
        uint8 second;
    }

    mapping(uint256 => PackedItem) packedItems;
    mapping(uint256 => PackedItem) otherPackedItems;
    uint8 yulPrefix;
    uint8 yulPacked;

    function bindAfterDeclaration(uint256 key, uint256 value)
        external
        returns (uint256, uint256)
    {
        Item storage item;
        item = items[key];
        item.a = value;
        item.b = value + 1;
        return (item.a, readB(item));
    }

    function rebind(bool useSecond, uint256 first, uint256 second, uint256 value)
        external
        returns (uint256, uint256)
    {
        Item storage item = items[first];
        if (useSecond) {
            item = items[second];
        }
        item.a = value;
        return (items[first].a, items[second].a);
    }

    function rebindParameter(uint256 first, uint256 second, uint256 value)
        external
        returns (uint256, uint256)
    {
        Item storage item = items[first];
        rebindParameterInner(item, second, value);
        return (items[first].a, items[second].a);
    }

    function swap(uint256 first, uint256 second, uint256 firstValue, uint256 secondValue)
        external
        returns (uint256, uint256)
    {
        Item storage firstItem = items[first];
        Item storage secondItem = items[second];
        (firstItem, secondItem) = (secondItem, firstItem);
        firstItem.a = firstValue;
        secondItem.a = secondValue;
        return (items[first].a, items[second].a);
    }

    function packedRebind(bool useSecond, uint256 first, uint256 second, uint8 value)
        external
        returns (uint8, uint8)
    {
        PackedItem storage item = packedItems[first];
        if (useSecond) {
            item = packedItems[second];
        }
        item.second = value;
        return (packedItems[first].second, packedItems[second].second);
    }

    function packedYulRebind(bool useSecond, uint256 first, uint256 second, uint256 value)
        external
        returns (uint256, uint256, uint256)
    {
        mapping(uint256 => PackedItem) storage itemsRef = packedItems;
        if (useSecond) {
            itemsRef = otherPackedItems;
        }
        PackedItem storage item = itemsRef[first];
        uint256 offset;
        assembly {
            offset := itemsRef.offset
            sstore(item.slot, value)
        }
        return (itemsRef[first].whole, itemsRef[second].whole, offset);
    }

    function yulPackedOffset(uint8 value) external returns (uint256 offset) {
        assembly {
            offset := yulPacked.offset
            let shift := mul(offset, 8)
            let mask := shl(shift, 0xff)
            sstore(yulPacked.slot, or(and(sload(yulPacked.slot), not(mask)), shl(shift, value)))
        }
    }

    function assignmentExpression(uint256 first, uint256 second, uint256 value)
        external
        returns (uint256, uint256)
    {
        Item storage item = items[first];
        (item = items[second]).a = value;
        return (items[first].a, items[second].a);
    }

    function mappingAssignmentExpression(uint256 first, uint256 second, uint8 value)
        external
        returns (uint8, uint8, uint8, uint8)
    {
        mapping(uint256 => PackedItem) storage itemsRef = packedItems;
        itemsRef[first].second = 1;
        (itemsRef = otherPackedItems)[second].second = value;
        return (
            packedItems[first].second,
            packedItems[second].second,
            otherPackedItems[first].second,
            otherPackedItems[second].second
        );
    }

    function readB(Item storage item) internal view returns (uint256) {
        return item.b;
    }

    function rebindParameterInner(Item storage item, uint256 key, uint256 value) internal {
        item = items[key];
        item.a = value;
    }
}
