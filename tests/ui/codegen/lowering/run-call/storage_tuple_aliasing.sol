//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: storageAliases() => 10, 10, 2, 1
//@ run-call: lhsMutation() => 99, 20
//@ run-call: memorySnapshot() => 10, 99
//@ run-call: returnedReferences() => 10, 10
//@ run-call: nestedReturnedReferences() => 10, 10, 20
//@ run-call: dynamicArrays() => 1, 1, 1, 1
//@ run-call: byteArrays() => 0x0102, 0x0102

contract StorageTupleAliasing {
    struct Item {
        uint256 value;
    }

    Item[3] private items;
    uint256 private lhsIndex;
    uint256 private rhsIndex;
    uint256[] private x;
    uint256[] private y;
    bytes private firstBytes;
    bytes private secondBytes;

    function storageAliases() external returns (uint256, uint256, uint256, uint256) {
        items[0].value = 10;
        items[1].value = 20;
        lhsIndex = 0;
        rhsIndex = 1;
        (items[nextLhs()], items[nextLhs()]) = (items[nextRhs()], items[nextRhs()]);
        return (items[0].value, items[1].value, lhsIndex, rhsIndex);
    }

    function lhsMutation() external returns (uint256, uint256) {
        items[0].value = 10;
        items[1].value = 20;
        (items[mutateSource()], items[1]) = (items[0], items[1]);
        return (items[0].value, items[1].value);
    }

    function memorySnapshot() external returns (uint256, uint256) {
        items[0].value = 10;
        Item memory copy;
        uint256[1] memory targets;
        // Storage-to-memory copies stay eager; only storage-to-storage copies are deferred.
        (copy, targets[mutateSource()]) = (items[0], 1);
        return (copy.value, items[0].value);
    }

    function returnedReferences() external returns (uint256, uint256) {
        items[0].value = 10;
        items[1].value = 20;
        (items[0], items[1]) = pair();
        return (items[0].value, items[1].value);
    }

    function nestedReturnedReferences() external returns (uint256, uint256, uint256) {
        items[0].value = 10;
        items[1].value = 20;
        ((items[0], items[1]), items[2]) = (pair(), items[1]);
        return (items[0].value, items[1].value, items[2].value);
    }

    function dynamicArrays() external returns (uint256, uint256, uint256, uint256) {
        delete x;
        delete y;
        x.push(1);
        y.push(2);
        y.push(3);
        (x, y) = (y, x);
        return (x.length, x[0], y.length, y[0]);
    }

    function byteArrays() external returns (bytes memory, bytes memory) {
        firstBytes = hex"0102";
        secondBytes = hex"030405";
        (firstBytes, secondBytes) = (secondBytes, firstBytes);
        return (firstBytes, secondBytes);
    }

    function nextLhs() private returns (uint256 index) {
        index = lhsIndex++;
    }

    function nextRhs() private returns (uint256 index) {
        index = rhsIndex;
        rhsIndex ^= 1;
    }

    function mutateSource() private returns (uint256) {
        items[0].value = 99;
        return 0;
    }

    function pair() private view returns (Item storage first, Item storage second) {
        first = items[1];
        second = items[0];
    }
}
