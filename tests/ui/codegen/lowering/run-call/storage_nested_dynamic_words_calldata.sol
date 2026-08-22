//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: copyDynamic(uint256[][]) [[10, 11], [20, 21, 22]] => 33
//@[gas] run-call: copyDynamic(uint256[][]) [[10, 11], [20, 21, 22]] => 33
//@[size] run-call: copyDynamic(uint256[][]) [[10, 11], [20, 21, 22]] => 33
//@[none] run-call: copyFixedDynamic(uint256[][2]) [[10, 11], [20, 21]] => 33
//@[gas] run-call: copyFixedDynamic(uint256[][2]) [[10, 11], [20, 21]] => 33
//@[size] run-call: copyFixedDynamic(uint256[][2]) [[10, 11], [20, 21]] => 33
//@[none] run-call: copyDynamicFixed(uint256[2][]) [[10, 11], [20, 21]] => 33
//@[gas] run-call: copyDynamicFixed(uint256[2][]) [[10, 11], [20, 21]] => 33
//@[size] run-call: copyDynamicFixed(uint256[2][]) [[10, 11], [20, 21]] => 33
//@[none] run-call: copyFixed(uint256[2][2]) [[10, 11], [20, 21]] => 31
//@[gas] run-call: copyFixed(uint256[2][2]) [[10, 11], [20, 21]] => 31
//@[size] run-call: copyFixed(uint256[2][2]) [[10, 11], [20, 21]] => 31
// ported-from: test/libsolidity/semanticTests/array/copying/nested_array_calldata_to_storage.sol

pragma abicoder v2;

contract StorageNestedDynamicWordsCalldata {
    uint256[][] private dynamicValues;
    uint256[][2] private fixedOuter;
    uint256[2][] private dynamicOuterFixed;
    uint256[2][2] private fixedValues;

    function copyDynamic(uint256[][] calldata input) external returns (uint256) {
        dynamicValues = input;
        return dynamicValues.length + dynamicValues[0][1] + dynamicValues[1][0];
    }

    function copyFixedDynamic(uint256[][2] calldata input) external returns (uint256) {
        fixedOuter = input;
        return fixedOuter[0].length + fixedOuter[0][1] + fixedOuter[1][0];
    }

    function copyDynamicFixed(uint256[2][] calldata input) external returns (uint256) {
        dynamicOuterFixed = input;
        return dynamicOuterFixed.length + dynamicOuterFixed[0][1] + dynamicOuterFixed[1][0];
    }

    function copyFixed(uint256[2][2] calldata input) external returns (uint256) {
        fixedValues = input;
        return fixedValues[0][0] + fixedValues[1][1];
    }
}
