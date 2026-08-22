//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: ConstructorModifierParameterFrame::observed() => 6

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
