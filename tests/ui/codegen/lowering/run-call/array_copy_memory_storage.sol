//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: copyDynamic => 1
//@ run-call: copyFixed => 6
// ported-from: test/libsolidity/semanticTests/array/copying/array_copy_memory_to_storage.sol

contract ArrayCopyMemoryStorage {
    uint32[] dynamicValues;
    uint32[3] fixedValues;

    function copyDynamic() external returns (uint32) {
        uint32[] memory values = new uint32[](3);
        values[0] = 1;
        values[1] = 2;
        values[2] = 3;
        dynamicValues = values;
        return dynamicValues[0];
    }

    function copyFixed() external returns (uint32) {
        uint32[3] memory values;
        values[0] = 1;
        values[1] = 2;
        values[2] = 3;
        dynamicValues = values;
        fixedValues = values;
        return dynamicValues[0] + fixedValues[1] + dynamicValues[2];
    }
}
