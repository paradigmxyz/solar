//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: C::getData => 6
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_for_constructor.sol

contract A {
    uint256 data;

    constructor() mod1 {
        data |= 2;
    }

    modifier mod1 virtual {
        data |= 1;
        _;
    }

    function getData() public returns (uint256 r) {
        //~^ WARN: function state mutability can be restricted to view
        return data;
    }
}

contract C is A {
    modifier mod1 override {
        data |= 4;
        _;
    }
}
