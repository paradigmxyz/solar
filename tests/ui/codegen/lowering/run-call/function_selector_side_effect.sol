//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: read() => 42
// ported-from: test/libsolidity/semanticTests/functionTypes/selector_expression_side_effect.sol

contract FunctionSelectorSideEffect {
    uint256 private value;

    function f() external view {}

    function h() external returns (FunctionSelectorSideEffect) {
        value = 42;
        return this;
    }

    function read() external returns (uint256) {
        h().f.selector;
        return value;
    }
}
