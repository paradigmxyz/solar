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
//@[none, gas, size] run-call: rebindLoop false, 3, 5, 17 => 17, 0
//@[none, gas, size] run-call: rebindLoop true, 3, 5, 17 => 0, 17

contract StorageReferenceLoop {
    struct Item {
        uint256 value;
    }

    mapping(uint256 => Item) items;

    function rebindLoop(bool useSecond, uint256 first, uint256 second, uint256 value)
        external
        returns (uint256, uint256)
    {
        Item storage item = items[first];
        for (uint256 i = 0; i < 1; i++) {
            if (useSecond) {
                item = items[second];
            }
        }
        item.value = value;
        return (items[first].value, items[second].value);
    }
}
