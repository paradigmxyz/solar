//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f false => 1
//@ run-call: f true => 1
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_multi_invocation_viair.sol

contract ModifierRepeatedPlaceholder {
    modifier repeat(bool twice) {
        if (twice) _;
        _;
    }

    function f(bool twice) external pure repeat(twice) returns (uint256 r) {
        r += 1;
    }
}
