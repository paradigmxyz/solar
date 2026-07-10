// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_stricter_mutability.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_stricter_mutability2.sol
// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_stricter_mutability3.sol

contract ViewBase {
    function foo() internal view virtual returns (uint256) {}
}
contract PureOverridesView is ViewBase {
    function foo() internal pure override virtual returns (uint256) {}
}
contract ViewOverridesView is ViewBase {
    function foo() internal view override virtual returns (uint256) {}
}
contract DiamondPure is PureOverridesView, ViewOverridesView {
    function foo() internal pure override(PureOverridesView, ViewOverridesView) virtual returns (uint256) {}
}
contract DiamondPureReversed is ViewOverridesView, PureOverridesView {
    function foo() internal pure override(PureOverridesView, ViewOverridesView) virtual returns (uint256) {}
}

contract NonpayableBase {
    function bar() internal virtual returns (uint256) {}
}
contract ViewOverridesNonpayable is NonpayableBase {
    function bar() internal view override virtual returns (uint256) {}
}

contract SecondViewBase {
    function baz() internal view virtual returns (uint256) {}
}
contract SecondPureOverridesView is SecondViewBase {
    function baz() internal pure override virtual returns (uint256) {}
}
