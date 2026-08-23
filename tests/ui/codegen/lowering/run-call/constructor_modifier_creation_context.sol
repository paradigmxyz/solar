//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: C::getData() => 0x4300
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_calling_functions_in_creation_context.sol

contract ConstructorModifierBase {
    uint256 internal data;

    constructor() mod1 {
        f1();
    }

    function f1() public mod2 {
        data |= 0x1;
    }

    function f2() public {
        data |= 0x20;
    }

    function f3() public virtual {}

    modifier mod1 virtual {
        f2();
        _;
    }

    modifier mod2 {
        f3();
        if (false) _;
    }

    function getData() public view returns (uint256) {
        return data;
    }
}

contract C is ConstructorModifierBase {
    modifier mod1 override {
        f4();
        _;
    }

    function f3() public override {
        data |= 0x300;
    }

    function f4() public {
        data |= 0x4000;
    }
}
