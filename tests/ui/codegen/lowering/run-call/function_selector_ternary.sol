//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: h(bool) true => 0x26121ff0
//@ run-call: h(bool) false => 0xe2179b8e
// ported-from: test/libsolidity/semanticTests/functionTypes/selector_ternary.sol

contract FunctionSelectorTernary {
    function f() external {}

    function g() external {}

    function h(bool condition) external view returns (bytes4) {
        return (condition ? this.f : this.g).selector;
    }
}
