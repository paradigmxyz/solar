// Tests for override mutability permutations (error code: 6959)
// Based on solc tests: override_stricter_mutability*.sol, override_less_strict_mutability.sol

// ==== Valid: pure can override view ====
contract ViewBase {
    function foo() internal view virtual returns (uint256) {}
}
contract PureOverridesView is ViewBase {
    function foo() internal pure override virtual returns (uint256) {}
}

// ==== Valid: view can override view ====
contract ViewOverridesView is ViewBase {
    function foo() internal view override virtual returns (uint256) {}
}

// ==== Valid: diamond with pure overriding both pure and view ====
contract DiamondPureA is ViewBase {
    function foo() internal pure override virtual returns (uint256) {}
}
contract DiamondViewC is ViewBase {
    function foo() internal view override virtual returns (uint256) {}
}
contract DiamondD is DiamondPureA, DiamondViewC {
    function foo() internal pure override(DiamondPureA, DiamondViewC) virtual returns (uint256) {}
}
contract DiamondE is DiamondViewC, DiamondPureA {
    function foo() internal pure override(DiamondPureA, DiamondViewC) virtual returns (uint256) {}
}

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

// ==== Invalid: pure -> view (less strict) ====
contract PureBase {
    function foo() external pure virtual returns (uint256) {}
}
contract Bad6 is PureBase {
    function foo() external view override virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `pure` to `view`
}

// ==== Invalid: multiple less strict overrides ====
contract PureViewDiamondB is PureBase {
    function foo() external pure override virtual returns (uint256) {}
}
contract PureViewDiamondC is PureBase {
    function foo() external view override virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `pure` to `view`
}
contract Bad7 is PureViewDiamondB, PureViewDiamondC {
    // nonpayable is less strict than both pure and view
    function foo() external override(PureViewDiamondB, PureViewDiamondC) virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `pure` to `nonpayable`
    //~| ERROR: overriding function changes state mutability from `view` to `nonpayable`
}

// ==== Valid: pure overriding both ====
contract Good1 is PureViewDiamondC, PureViewDiamondB {
    function foo() external pure override(PureViewDiamondB, PureViewDiamondC) virtual returns (uint256) {}
}

// ==== Invalid: payable is less strict than everything ====
contract Bad8 is PureViewDiamondC, PureViewDiamondB {
    function foo() external payable override(PureViewDiamondB, PureViewDiamondC) virtual returns (uint256) {}
    //~^ ERROR: overriding function changes state mutability from `view` to `payable`
    //~| ERROR: overriding function changes state mutability from `pure` to `payable`
}
