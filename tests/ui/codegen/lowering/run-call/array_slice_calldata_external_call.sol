//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: forward [1, 2, 3, 4], 1, 3 => 2
// ported-from: test/libsolidity/semanticTests/array/slices/array_slice_calldata_as_argument_of_external_calls.sol

contract ArraySliceCalldataExternalCall {
    function sink(uint256[] calldata values) external pure returns (uint256) {
        return values.length;
    }

    function forward(uint256[] calldata values, uint256 start, uint256 end)
        external
        view
        returns (uint256)
    {
        return this.sink(values[start:end]);
    }
}
