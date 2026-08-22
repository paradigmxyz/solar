//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: copy => 207
// ported-from: test/libsolidity/semanticTests/array/copying/storage_memory_nested_bytes.sol

contract StorageMemoryNestedBytes {
    bytes[] values;

    function copy() external returns (uint256) {
        values.push(hex"616263");
        bytes memory longValue = new bytes(40);
        longValue[39] = "B";
        values.push(longValue);

        bytes[] memory copied = values;
        return copied[0].length + copied[1].length + uint8(copied[0][1])
            + uint8(copied[1][39]);
    }
}
