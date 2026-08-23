//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
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
