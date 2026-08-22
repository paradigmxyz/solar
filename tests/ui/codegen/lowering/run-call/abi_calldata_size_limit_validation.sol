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
//@[none, gas, size] run-call: f 6, [1, 2], 9 => 7
//@[none, gas, size] run-call-fail: 0x975af906000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000009800000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002
// ported-from: test/libsolidity/semanticTests/abicoder/validation/array_exceeds_size_limit_for_calldata_types_v2.sol

pragma abicoder v2;

contract AbiCalldataSizeLimitValidation {
    function f(uint256 a, uint256[] calldata, uint256 c) external pure returns (uint256) {
        return a + c - 8;
    }
}
