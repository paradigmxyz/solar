//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f() => false
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
