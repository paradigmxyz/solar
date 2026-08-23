//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: C::f() => false
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_overriding.sol

contract A {
    function f() public mod returns (bool r) {
        //~^ WARN: function state mutability can be restricted to pure
        return true;
    }

    modifier mod virtual {
        _;
    }
}

contract C is A {
    modifier mod override {
        if (false) _;
    }
}
