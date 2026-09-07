//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: copy [1, 2, 3, 4], 1, 3 => 2
//@ run-call: forward [1, 2, 3, 4], 1, 3 => 2
//@ run-call: literal [1, 2, 3, 4], 1, 3 => 2
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

    // A slice mobilizes to its memory array type, so it can be an inline array element.
    function literal(uint256[] calldata values, uint256 start, uint256 end)
        external
        pure
        returns (uint256)
    {
        return [values[start:end]][0][0];
    }
}
