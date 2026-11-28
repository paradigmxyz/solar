//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: ConstructorModifierOverride::getData => 7
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_for_constructor.sol

contract ConstructorModifierOverrideBase {
    uint256 data;

    constructor() m {
        data |= 2;
    }

    modifier m {
        data |= 1;
        _;
    }

    function getData() external view returns (uint256) {
        return data;
    }
}

contract ConstructorModifierOverride is ConstructorModifierOverrideBase {
    constructor() {
        data |= 4;
    }
}
