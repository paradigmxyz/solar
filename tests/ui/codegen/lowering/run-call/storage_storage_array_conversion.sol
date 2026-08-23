//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: dynamic => 9
//@ run-call: nested => 7
//@ run-call: fixedToDynamic => 13
// ported-from: test/libsolidity/semanticTests/array/copying/array_copy_storage_storage_different_base.sol
// ported-from: test/libsolidity/semanticTests/array/copying/array_copy_storage_storage_different_base_nested.sol

contract StorageStorageArrayConversion {
    uint64[] dynamicSource;
    uint256[] dynamicTarget;
    uint256[9] fixedSource;
    uint256[] fixedDynamicTarget;
    uint48[5][2] nestedSource;
    uint120[6][3] nestedTarget;

    function dynamic() external returns (uint256) {
        dynamicSource.push(0);
        dynamicSource.push(1);
        dynamicSource.push(2);
        dynamicSource.push(3);
        dynamicSource.push(4);
        dynamicTarget.push(11);
        dynamicTarget = dynamicSource;
        return dynamicTarget.length + dynamicTarget[4];
    }

    function nested() external returns (uint256) {
        nestedTarget[0][0] = 11;
        nestedTarget[1][0] = 22;
        nestedTarget[2][0] = 33;
        nestedSource[0][0] = 0;
        nestedSource[0][1] = 1;
        nestedSource[0][2] = 2;
        nestedSource[0][3] = 3;
        nestedSource[0][4] = 4;
        nestedSource[1][0] = 3;
        nestedTarget = nestedSource;
        return 3 + nestedTarget[0][4];
    }

    function fixedToDynamic() external returns (uint256) {
        fixedSource[8] = 4;
        fixedDynamicTarget = fixedSource;
        return fixedDynamicTarget.length + fixedDynamicTarget[8];
    }
}
