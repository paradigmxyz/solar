//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: copy [1, 2, 3, 4], 1, 3 => 2
//@[none, gas, size] run-call: forward [1, 2, 3, 4], 1, 3 => 2
// ported-from: test/libsolidity/semanticTests/array/slices/array_slice_calldata_to_memory.sol

contract ArraySliceCalldataToMemory {
    function copy(uint256[] calldata values, uint256 start, uint256 end)
        external
        pure
        returns (uint256)
    {
        uint256[] memory copied = values[start:end];
        return copied[0];
    }

    function first(uint256[] memory values) internal pure returns (uint256) {
        return values[0];
    }

    function forward(uint256[] calldata values, uint256 start, uint256 end)
        external
        pure
        returns (uint256)
    {
        return first(values[start:end]);
    }
}
