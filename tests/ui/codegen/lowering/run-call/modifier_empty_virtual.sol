//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: f() => false
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_empty.sol

abstract contract ModifierEmptyBase {
    function f() external mod returns (bool result) {
        //~^ WARN: function state mutability can be restricted to pure
        result = true;
    }

    modifier mod virtual;
}

contract ModifierEmptyVirtual is ModifierEmptyBase {
    modifier mod override {
        if (false) _;
    }
}
