//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f [] => 7
//@ run-call-fail: 0x7bc5bbbf00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: 0x7bc5bbbf00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000002
// ported-from: test/libsolidity/semanticTests/abicoder/validation/array_exceeds_calldatasize_v2.sol

pragma abicoder v2;

contract AbiCalldataArraySizeValidation {
    function f(uint256[] calldata) external pure returns (uint256) {
        return 7;
    }
}
