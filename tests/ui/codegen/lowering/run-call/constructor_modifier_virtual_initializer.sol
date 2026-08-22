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
//@[none] run-call: ConstructorModifierVirtualInitializer::observed() => 0
//@[gas] run-call: ConstructorModifierVirtualInitializer::observed() => 0
//@[size] run-call: ConstructorModifierVirtualInitializer::observed() => 0

contract ConstructorModifierVirtualInitializerBase {
    uint256 public observed;

    constructor() record(value()) {}

    modifier record(uint256 value_) {
        observed = value_;
        _;
    }

    function value() internal virtual returns (uint256) {
        return 1;
    }
}

contract ConstructorModifierVirtualInitializer
    is ConstructorModifierVirtualInitializerBase
{
    uint256 public valueFromInitializer = 42;

    function value() internal view override returns (uint256) {
        return valueFromInitializer;
    }
}
