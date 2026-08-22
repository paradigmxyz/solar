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
//@[none, gas, size] run-call: f() => 2, 3
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_return_reference.sol

contract ModifierReturnReference {
    modifier setX(uint256 value) {
        _;
    }

    modifier setY(uint256 value) {
        _;
    }

    function f() public setX(x = 2) setY(y = 3) returns (uint256 x, uint256 y) {}
}
