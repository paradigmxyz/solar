//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: C::getData() => 0x4300
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_calling_functions_in_creation_context.sol

contract A {
    uint256 data;

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

    function getData() public view returns (uint256 r) {
        return data;
    }
}

contract C is A {
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
