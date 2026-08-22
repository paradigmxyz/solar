//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: read [42] => 42
//@[none, gas, size] run-call: encode [42] => 96
//@[none, gas, size] run-call: unused [42] => 1
//@[none, gas, size] run-call-fail: 0x9a15a5b800000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000002a
//@[none, gas, size] run-call-fail: 0xb003dd8600000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000002a
// ported-from: test/libsolidity/semanticTests/abicoder/validation/calldata_with_garbage_v2.sol

pragma abicoder v2;

contract AbiCalldataStrideValidation {
    function read(uint256[] calldata values) external pure returns (uint256) {
        return values[0];
    }

    function encode(uint256[] calldata values) external pure returns (uint256) {
        return abi.encode(values).length;
    }

    function unused(uint256[] calldata) external pure returns (uint256) {
        return 1;
    }
}
