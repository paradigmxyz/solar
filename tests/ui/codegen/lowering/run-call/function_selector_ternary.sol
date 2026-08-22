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
//@[none] run-call: h(bool) true => 0x26121ff0
//@[gas] run-call: h(bool) true => 0x26121ff0
//@[size] run-call: h(bool) true => 0x26121ff0
//@[none] run-call: h(bool) false => 0xe2179b8e
//@[gas] run-call: h(bool) false => 0xe2179b8e
//@[size] run-call: h(bool) false => 0xe2179b8e
// ported-from: test/libsolidity/semanticTests/functionTypes/selector_ternary.sol

contract FunctionSelectorTernary {
    function f() external {}

    function g() external {}

    function h(bool condition) external view returns (bytes4) {
        return (condition ? this.f : this.g).selector;
    }
}
