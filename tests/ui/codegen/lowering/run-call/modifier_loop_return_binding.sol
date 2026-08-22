//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
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
