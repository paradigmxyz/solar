// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_less_strict_mutability.sol

contract A {
    function foo() external pure virtual returns (uint256) {}
}
contract B is A {
    function foo() external pure override virtual returns (uint256) {}
}
contract C is A {
    function foo() external view override virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `pure` to `view`
}
contract D is B, C {
    function foo() external override(B, C) virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `pure` to `nonpayable`
    //~| ERROR: overriding function changes state mutability from `view` to `nonpayable`
}
contract E is C, B {
    function foo() external pure override(B, C) virtual returns (uint256) {}
}
contract F is C, B {
    function foo() external payable override(B, C) virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `view` to `payable`
    //~| ERROR: overriding function changes state mutability from `pure` to `payable`
}
