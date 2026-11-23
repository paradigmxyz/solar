pragma solidity ^0.8.20;
// Two alike declarations of `Item` would share the invariant only one states. A state variable
// holds one, so the file is refused.
contract A {
    /// @custom:invariant v > 0
    struct Item { uint256 v; }
}
contract B {
    struct Item { uint256 v; }
    Item private it;
    /// @custom:ensures it.v > 0
    function f() external view {}
}
