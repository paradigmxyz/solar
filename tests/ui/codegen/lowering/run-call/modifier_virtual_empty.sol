//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f() => false
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_empty.sol

abstract contract ModifierVirtualEmptyBase {
    function f() public pure mod returns (bool r) {
        return true;
    }

    modifier mod virtual;
}

contract ModifierVirtualEmpty is ModifierVirtualEmptyBase {
    modifier mod override {
        if (false) _;
    }
}
