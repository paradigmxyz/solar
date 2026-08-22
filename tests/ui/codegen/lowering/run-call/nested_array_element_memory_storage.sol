//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: test [[[1, 2], [3, 4]], [[5, 6], [7, 8]]] => 10
//@ run-call: test2 [[1, 2], [3, 4]] => 10
// ported-from: test/libsolidity/semanticTests/array/copying/nested_array_element_memory_to_storage.sol

contract NestedArrayElementMemoryStorage {
    uint8[2][2] first;
    uint8[2][2][2] second;

    function test(uint8[2][2][2] memory values) external returns (uint256) {
        first = values[0];
        return first[0][0] + first[0][1] + first[1][0] + first[1][1];
    }

    function test2(uint8[2][2] memory values) external returns (uint256) {
        second[0] = values;
        return second[0][0][0] + second[0][0][1] + second[0][1][0] + second[0][1][1];
    }
}
