//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: assignment 42 => 42
//@ run-call: newValue 42 => 42
// ported-from: test/libsolidity/semanticTests/array/array_3d_assignment.sol
// ported-from: test/libsolidity/semanticTests/array/array_3d_new.sol

contract MemoryNestedArrays {
    function assignment(uint256 value) external pure returns (uint256) {
        uint256[][][] memory values = new uint256[][][](2);
        for (uint256 i; i < 2; ++i) {
            values[i] = new uint256[][](3);
            for (uint256 j; j < 3; ++j) {
                values[i][j] = new uint256[](4);
            }
        }
        values[1][1][1] = value;
        uint256[][] memory row = values[1];
        uint256[] memory column = row[1];
        return column[1];
    }

    function newValue(uint256 value) external pure returns (uint256) {
        uint256[][][] memory values = new uint256[][][](2);
        for (uint256 i; i < 2; ++i) {
            values[i] = new uint256[][](3);
            for (uint256 j; j < 3; ++j) {
                values[i][j] = new uint256[](4);
            }
        }
        return values[1][1][1] = value;
    }
}
