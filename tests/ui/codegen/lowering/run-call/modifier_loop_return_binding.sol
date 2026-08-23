//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: f => 1
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_loop_viair.sol

contract ModifierLoopReturnBinding {
    modifier repeat(uint256 count) {
        uint256 i;
        for (i = 0; i < count; ++i) _;
    }

    function f() external pure repeat(10) returns (uint256 r) {
        r += 1;
    }
}
