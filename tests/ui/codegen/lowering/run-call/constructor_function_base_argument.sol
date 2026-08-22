//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: ConstructorFunctionBaseDerived::getA() => 2
// ported-from: test/libsolidity/semanticTests/constructor/function_usage_in_constructor_arguments.sol

contract ConstructorFunctionBaseBase {
    uint256 value;

    constructor(uint256 value_) {
        value = value_;
    }

    function two() public pure returns (uint256) {
        return 2;
    }
}

contract ConstructorFunctionBase is ConstructorFunctionBaseBase(ConstructorFunctionBaseBase.two()) {}

contract ConstructorFunctionBaseDerived is ConstructorFunctionBase {
    function getA() external view returns (uint256) {
        return value;
    }
}
