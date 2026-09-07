//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test1 => [[1, 2], [3, 4, 5]]
//@ run-call: test2 => [[6, 7, 8], [9]]
//@ run-call: test3 => [[1, 2], [3, 4], [5, 6]]
//@ run-call: test4 => [[10, 11], [12, 13]]
// ported-from: test/libsolidity/semanticTests/array/copying/nested_array_storage_to_memory.sol

pragma abicoder v2;

contract NestedArrayStorageMemory {
    uint256[][] a1;
    uint256[][2] a2;
    uint256[2][] a3;
    uint256[2][2] a4;

    constructor() {
        a1 = new uint256[][](2);
        a1[0] = [1, 2];
        a1[1] = [3, 4, 5];

        a2[0] = [6, 7, 8];
        a2[1] = [9];

        a3.push([1, 2]);
        a3.push([3, 4]);
        a3.push([5, 6]);

        a4 = [[10, 11], [12, 13]];
    }

    function test1() external view returns (uint256[][] memory) {
        return a1;
    }

    function test2() external view returns (uint256[][2] memory) {
        return a2;
    }

    function test3() external view returns (uint256[2][] memory) {
        return a3;
    }

    function test4() external view returns (uint256[2][2] memory) {
        return a4;
    }
}
