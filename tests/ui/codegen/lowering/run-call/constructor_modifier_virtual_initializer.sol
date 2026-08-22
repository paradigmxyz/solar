//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: ConstructorModifierVirtualInitializer::observed() => 0

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
