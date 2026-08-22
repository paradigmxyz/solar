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
//@[none, gas, size] run-call: encode [7, 42] => 128
//@[none, gas, size] run-call-fail: 0x1597ee4400000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000ff23000000000000000000000000000000000000000000000000000000000000002a
//@[none, gas, size] run-call: read [7, 42] => 1
//@[none, gas, size] run-call-fail: 0xfbb1e83c00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000ff23000000000000000000000000000000000000000000000000000000000000002a
// ported-from: test/libsolidity/semanticTests/abicoder/calldataDecoding/array/calldata_array_dynamic_of_value_types_v2.sol
// ported-from: test/libsolidity/semanticTests/abicoder/cleanup/dynamic_array_v2.sol

pragma abicoder v2;

contract AbiCalldataScalarValidation {
    function encode(uint8[] calldata values) external pure returns (uint256) {
        return abi.encode(values).length;
    }

    function read(uint8[] calldata values) external pure returns (uint256) {
        values[0];
        return 1;
    }
}
