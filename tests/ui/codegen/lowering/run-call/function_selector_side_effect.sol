//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: read() => 42
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
