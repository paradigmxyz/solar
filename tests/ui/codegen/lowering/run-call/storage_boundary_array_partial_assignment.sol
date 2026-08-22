//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: test() => 36
// ported-from: test/libsolidity/semanticTests/storage/storage_boundary_array_partial_assignment.sol

contract StorageBoundaryArrayPartialAssignment {
    function getArray() internal pure returns (uint256[10][1] storage array) {
        assembly {
            array.slot := sub(0, 5)
        }
    }

    function test() public returns (uint256 sum) {
        uint256[10][1] storage array = getArray();
        array[0] = [11, 12, 13];
        for (uint256 i = 0; i < array[0].length; ++i) sum += array[0][i];
    }
}
