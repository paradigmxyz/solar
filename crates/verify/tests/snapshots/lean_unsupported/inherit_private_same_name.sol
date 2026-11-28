// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
// A base's private variable and the derived contract's share the name `x`: the base's spec means
// its own `x` (slot 0), which a lookup by name in `Derived`'s layout read at the derived
// contract's slot (slot 1).
contract Base {
    uint256 private x;
    // expect: `x` names more than one state variable in `Derived`'s hierarchy
    /// @custom:ensures x == v
    function setBase(uint256 v) external { x = v; }
}
contract Derived is Base {
    uint256 public x;
}
