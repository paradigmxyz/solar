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
//@[none] run-call: copy => 207
//@[gas] run-call: copy => 207
//@[size] run-call: copy => 207
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
