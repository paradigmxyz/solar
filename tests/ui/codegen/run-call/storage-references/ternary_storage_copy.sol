//@ run-call: TernaryStorageCopy::bytesCopy true => 0xaabb
//@ run-call: TernaryStorageCopy::bytesCopy false => 0xcc
//@ run-call: TernaryStorageCopy::dynamicCopy true => 2, 11
//@ run-call: TernaryStorageCopy::dynamicCopy false => 1, 22
//@ run-call: TernaryStorageCopy::fixedCopy true => 1, 2
//@ run-call: TernaryStorageCopy::fixedCopy false => 3, 4
//@ run-call: TernaryStorageCopy::structCopy true => 1, 0xaa
//@ run-call: TernaryStorageCopy::structCopy false => 2, 0xbbcc
//@ run-call: TernaryStorageCopy::nestedCopy true, false => 3
//@ run-call: TernaryStorageCopy::nestedCopy false, true => 5
//@ run-call: TernaryStorageCopy::memoryCalldata true, [6] => 6
//@ run-call: TernaryStorageCopy::memoryCalldata false, [6] => 7
//@ run-call: TernaryStorageCopy::storageMemory true => 8
//@ run-call: TernaryStorageCopy::storageMemory false => 9
contract TernaryStorageCopy {
    struct Item {
        uint256 value;
        bytes data;
    }

    bytes private blob;
    uint256[] private dynamicArray;
    uint256[2] private fixedArray;
    Item private item;
    uint256[] private leftStorage;

    function bytesCopy(bool condition) external returns (bytes memory) {
        bytes memory a = hex"aabb";
        bytes memory b = hex"cc";
        blob = condition ? a : b;
        return blob;
    }

    function dynamicCopy(bool condition) external returns (uint256, uint256) {
        uint256[] memory a = new uint256[](2);
        uint256[] memory b = new uint256[](1);
        a[1] = 11;
        b[0] = 22;
        dynamicArray = condition ? a : b;
        return (dynamicArray.length, dynamicArray[dynamicArray.length - 1]);
    }

    function fixedCopy(bool condition) external returns (uint256, uint256) {
        uint256[2] memory a = [uint256(1), 2];
        uint256[2] memory b = [uint256(3), 4];
        fixedArray = condition ? a : b;
        return (fixedArray[0], fixedArray[1]);
    }

    function structCopy(bool condition) external returns (uint256, bytes memory) {
        Item memory a = Item(1, hex"aa");
        Item memory b = Item(2, hex"bbcc");
        item = condition ? a : b;
        return (item.value, item.data);
    }

    function nestedCopy(bool outer, bool inner) external returns (uint256) {
        uint256[2] memory a = [uint256(1), 2];
        uint256[2] memory b = [uint256(3), 4];
        uint256[2] memory c = [uint256(5), 6];
        fixedArray = outer ? (inner ? a : b) : c;
        return fixedArray[0];
    }

    function memoryCalldata(bool condition, uint256[] calldata a) external returns (uint256) {
        uint256[] memory b = new uint256[](1);
        b[0] = 7;
        dynamicArray = condition ? a : b;
        return dynamicArray[0];
    }

    function storageMemory(bool condition) external returns (uint256) {
        leftStorage.push(8);
        uint256[] memory b = new uint256[](1);
        b[0] = 9;
        dynamicArray = condition ? leftStorage : b;
        return dynamicArray[0];
    }
}
