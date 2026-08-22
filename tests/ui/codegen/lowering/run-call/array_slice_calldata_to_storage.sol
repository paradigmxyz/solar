//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: copy [1, 2, 3, 4], 1, 3 => 2
// ported-from: test/libsolidity/semanticTests/array/slices/array_slice_calldata_to_storage.sol

contract ArraySliceCalldataToStorage {
    int256[] private stored;

    function copy(int256[] calldata values, uint256 start, uint256 end)
        external
        returns (int256)
    {
        stored = values[start:end];
        return stored[0];
    }
}
