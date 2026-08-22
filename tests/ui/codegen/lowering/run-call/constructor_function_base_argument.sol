//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: ConstructorFunctionBaseDerived::getA() => 2
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
