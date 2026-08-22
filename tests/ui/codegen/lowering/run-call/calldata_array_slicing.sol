//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: sliceLength [1, 2, 3, 4, 5], 2, 4 => 2
//@ run-call: sliceIndex [1, 2, 3, 4, 5], 2, 4, 1 => 4
//@ run-call: chainedIndex [1, 2, 3, 4, 5] => 3
//@ run-call: nestedSliceLength [1, 2, 3, 4, 5], 1, 4, 1, 2 => 1
//@ run-call: startSliceLength [1, 2, 3, 4, 5], 2 => 3
//@ run-call: endSliceLength [1, 2, 3, 4, 5], 3 => 3
//@ run-call: startSliceIndex [1, 2, 3, 4, 5], 2, 1 => 4
//@ run-call: endSliceIndex [1, 2, 3, 4, 5], 3, 2 => 3
//@ run-call-fail: sliceLength [1, 2, 3, 4, 5], 2, 6
//@ run-call-fail: sliceIndex [1, 2, 3, 4, 5], 2, 4, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: bytesSlice 0x010203, 0, 4
//@ run-call-fail: bytesSlice 0x010203, 2, 1
// ported-from: test/libsolidity/semanticTests/abicoder/calldataDecoding/array/calldata_array_slicing_v2.sol

contract CalldataArraySlicing {
    function sliceLength(uint256[] calldata values, uint256 start, uint256 end)
        external
        pure
        returns (uint256)
    {
        return uint256[](values[start:end]).length;
    }

    function sliceIndex(
        uint256[] calldata values,
        uint256 start,
        uint256 end,
        uint256 index
    ) external pure returns (uint256) {
        return values[start:end][index];
    }

    function chainedIndex(uint256[] calldata values) external pure returns (uint256) {
        return values[1:][1:][0];
    }

    function nestedSliceLength(
        uint256[] calldata values,
        uint256 start,
        uint256 end,
        uint256 nestedStart,
        uint256 nestedEnd
    ) external pure returns (uint256) {
        return uint256[](values[start:end][nestedStart:nestedEnd]).length;
    }

    function startSliceLength(uint256[] calldata values, uint256 start)
        external
        pure
        returns (uint256)
    {
        return uint256[](values[start:]).length;
    }

    function endSliceLength(uint256[] calldata values, uint256 end)
        external
        pure
        returns (uint256)
    {
        return uint256[](values[:end]).length;
    }

    function startSliceIndex(uint256[] calldata values, uint256 start, uint256 index)
        external
        pure
        returns (uint256)
    {
        return values[start:][index];
    }

    function endSliceIndex(uint256[] calldata values, uint256 end, uint256 index)
        external
        pure
        returns (uint256)
    {
        return values[:end][index];
    }

    function bytesSlice(bytes calldata values, uint256 start, uint256 end)
        external
        pure
        returns (bytes memory)
    {
        return values[start:end];
    }
}
