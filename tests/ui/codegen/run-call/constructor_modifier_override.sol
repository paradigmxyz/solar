//@ run-call: ConstructorModifierOverride::getData() => 6
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_for_constructor.sol

contract ConstructorModifierOverrideBase {
    uint256 data;

    constructor() mod1 {
        data |= 2;
    }

    modifier mod1 virtual {
        data |= 1;
        _;
    }

    function getData() external view returns (uint256) {
        return data;
    }
}

contract ConstructorModifierOverride is ConstructorModifierOverrideBase {
    modifier mod1 override {
        data |= 4;
        _;
    }
}
