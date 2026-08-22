//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: copy => 28
// ported-from: test/libsolidity/semanticTests/array/copying/storage_memory_nested_from_pointer.sol

contract NestedStorageMemoryPointerCopy {
    uint72[5][] values;

    function copy() external returns (uint256) {
        for (uint256 i = 0; i < 4; i++) values.push();
        values[0][0] = 1;
        values[0][3] = 2;
        values[1][1] = 3;
        values[1][4] = 4;
        values[2][0] = 5;
        values[3][2] = 6;
        values[3][3] = 7;

        uint72[5][] storage pointer = values;
        uint72[5][] memory copied = pointer;
        return copied[0][0] + copied[0][3] + copied[1][1] + copied[1][4]
            + copied[2][0] + copied[3][2] + copied[3][3];
    }
}
