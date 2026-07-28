//@ run-call: bindAfterDeclaration 7, 11 => 11, 12
//@ run-call: rebind true, 3, 5, 17 => 0, 17
//@ run-call: rebind false, 3, 5, 17 => 17, 0
//@ run-call: rebindParameter 3, 5, 19 => 0, 19
//@ run-call: swap 3, 5, 23, 29 => 29, 23

contract StorageReferenceReassignment {
    struct Item {
        uint256 a;
        uint256 b;
    }

    mapping(uint256 => Item) items;

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

    function readB(Item storage item) internal view returns (uint256) {
        return item.b;
    }

    function rebindParameterInner(Item storage item, uint256 key, uint256 value) internal {
        item = items[key];
        item.a = value;
    }
}
