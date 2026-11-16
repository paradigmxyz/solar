//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test1 [[[1], [2]], [[7, 8], [9]]] => 24
//@ run-call: test2 [[[1, 2], [3]], [[4, 5], [6]]] => 15
//@ run-call: test3 [[[1, 2], [3, 4]], [[5, 6]]] => 10
// ported-from: test/libsolidity/semanticTests/array/copying/nested_array_element_calldata_to_memory.sol

pragma abicoder v2;

contract CalldataNestedElementMemory {
    function test1(uint8[][][] calldata values) external pure returns (uint256) {
        uint8[][] memory selected = values[1];
        return selected[0][0] + selected[0][1] + selected[1][0];
    }

    function test2(uint8[][2][] calldata values) external pure returns (uint256) {
        uint8[][2] memory selected = values[1];
        return selected[0][0] + selected[0][1] + selected[1][0];
    }

    function test3(uint8[2][][2] calldata values) external pure returns (uint256) {
        uint8[2][] memory selected = values[0];
        return selected[0][0] + selected[0][1] + selected[1][0] + selected[1][1];
    }
}
