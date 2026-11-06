//@ codegen-matrix: standard
//@ run-call: yulBind 17 => 17, 18
//@ run-call: switchBind 0, 17 => 17
//@ run-call: switchBind 1, 19 => 19
//@ run-call: switchBind 2, 23 => 23
//@ run-call-fail: switchBind 3, 23 => 0x
//@ run-call: doWhileBind 17 => 17
//@ run-call: tryBind false, 17 => 17
//@ run-call: tryBind true, 19 => 19
//@ run-call: branchBind true, 17 => 17, 18, 0
//@ run-call: branchBind false, 17 => 0, 18, 17
//@ run-call: branchBindReturn true, 17 => 0
//@ run-call: branchBindReturn false, 17 => 18

contract StorageReferenceInitialization {
    struct Item {
        uint256 a;
        uint256 b;
    }

    mapping(uint256 => Item) private items;

    function yulBind(uint256 value) external returns (uint256, uint256) {
        Item storage item;
        assembly { item.slot := 42 }
        item.a = value;
        item.b = value + 1;
        return (item.a, readB(item));
    }

    function branchBind(bool first, uint256 value) external returns (uint256, uint256, uint256) {
        Item storage item;
        if (first) {
            item = items[1];
        } else {
            item = items[2];
        }
        item.a = value;
        item.b = value + 1;
        return (items[1].a, readB(item), items[2].a);
    }

    function branchBindReturn(bool stop, uint256 value) external returns (uint256) {
        Item storage item;
        if (stop) {
            return 0;
        } else {
            item = items[2];
        }
        item.b = value + 1;
        return readB(item);
    }

    function switchBind(uint256 which, uint256 value) external returns (uint256) {
        Item storage item;
        assembly {
            switch which
            case 0 { item.slot := 42 }
            case 1 { item.slot := 43 }
            case 3 { revert(0, 0) }
            default { item.slot := 44 }
        }
        item.b = value;
        return readB(item);
    }

    function doWhileBind(uint256 value) external returns (uint256) {
        Item storage item;
        do {
            item = items[1];
        } while (false);
        item.b = value;
        return readB(item);
    }

    function mayFail(bool fail) external pure {
        require(!fail);
    }

    function tryBind(bool fail, uint256 value) external returns (uint256) {
        Item storage item;
        try this.mayFail(fail) {
            item = items[1];
        } catch {
            item = items[2];
        }
        item.b = value;
        return readB(item);
    }

    function readB(Item storage item) internal view returns (uint256) {
        return item.b;
    }
}
