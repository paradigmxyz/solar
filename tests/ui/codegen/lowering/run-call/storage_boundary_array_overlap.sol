//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: test() => 42, 5, 0
// ported-from: test/libsolidity/semanticTests/storage/storage_boundary_array_delete_overlapping_variable.sol

contract StorageBoundaryArrayOverlap {
    uint256 public y = 42;

    function getArray() internal pure returns (uint256[10][1] storage arr) {
        assembly {
            arr.slot := sub(0, 5)
        }
    }

    function test() public returns (uint256 beforeFill, uint256 afterFill, uint256 afterClear) {
        uint256[10][1] storage arr = getArray();
        beforeFill = arr[0][5];
        for (uint256 i = 1; i < 10; ++i) {
            arr[0][i] = i;
        }
        afterFill = y;
        delete arr[0];
        afterClear = y;
    }
}
