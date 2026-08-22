//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: f [] => 7
//@[none, gas, size] run-call-fail: 0x7bc5bbbf00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001
//@[none, gas, size] run-call-fail: 0x7bc5bbbf00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000002
// ported-from: test/libsolidity/semanticTests/abicoder/validation/array_exceeds_calldatasize_v2.sol

pragma abicoder v2;

contract AbiCalldataArraySizeValidation {
    function f(uint256[] calldata) external pure returns (uint256) {
        return 7;
    }
}
