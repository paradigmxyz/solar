//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f => false
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
