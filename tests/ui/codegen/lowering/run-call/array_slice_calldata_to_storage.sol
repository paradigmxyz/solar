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
//@[none] run-call: copy [1, 2, 3, 4], 1, 3 => 2
//@[gas] run-call: copy [1, 2, 3, 4], 1, 3 => 2
//@[size] run-call: copy [1, 2, 3, 4], 1, 3 => 2
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
