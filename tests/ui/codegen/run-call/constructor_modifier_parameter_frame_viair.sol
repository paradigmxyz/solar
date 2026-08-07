//@ run-call: ConstructorModifierParameterFrame::observed() => 6

contract ConstructorModifierParameterFrameBase {
    uint256 public observed;

    constructor(uint256 x) repeat {
        x += 1;
        observed = x;
    }

    modifier repeat {
        _;
        _;
    }
}

contract ConstructorModifierParameterFrame is ConstructorModifierParameterFrameBase {
    constructor() ConstructorModifierParameterFrameBase(5) {}
}
