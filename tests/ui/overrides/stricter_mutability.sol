// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_stricter_mutability1.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_stricter_mutability4.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_stricter_mutability5.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_stricter_mutability6.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_stricter_mutability7.sol

// ==== Invalid: payable -> nonpayable ====
contract PayableBase1 {
    function foo() public payable virtual returns (uint256) {}
}
contract Bad1 is PayableBase1 {
    function foo() public override virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `payable` to `nonpayable`
}

// ==== Invalid: payable -> view ====
contract Bad2 is PayableBase1 {
    function foo() public view override virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `payable` to `view`
}

// ==== Invalid: payable -> pure ====
contract Bad3 is PayableBase1 {
    function foo() public pure override virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `payable` to `pure`
}

// ==== Invalid: nonpayable -> payable ====
contract NonpayableBase {
    function foo() public virtual returns (uint256) {}
}
contract Bad4 is NonpayableBase {
    function foo() public payable override virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `nonpayable` to `payable`
}

// ==== Invalid: view -> payable ====
contract ViewBase2 {
    function foo() public view virtual returns (uint256) {}
}
contract Bad5 is ViewBase2 {
    function foo() public payable override virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `view` to `payable`
}
