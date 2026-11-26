// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
// The derived contract's spec names its own `x`, which a base's private variable also names: the
// name alone does not say which declaration, and so which slot, it reads.
contract Base {
    uint256 private x;
    function setBase(uint256 v) external { x = v; }
}
contract Derived is Base {
    uint256 public x;
    // expect: `x` names more than one state variable in `Derived`'s hierarchy
    /// @custom:ensures x == v
    function set(uint256 v) external { x = v; }
}
