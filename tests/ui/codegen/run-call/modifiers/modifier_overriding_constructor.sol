//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: C::getData => 7
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_for_constructor.sol

contract A {
    uint256 data;

    constructor() m {
        data |= 2;
    }

    modifier m {
        data |= 1;
        _;
    }

    function getData() public returns (uint256 r) {
        //~^ WARN: function state mutability can be restricted to view
        return data;
    }
}

contract C is A {
    constructor() {
        data |= 4;
    }
}
