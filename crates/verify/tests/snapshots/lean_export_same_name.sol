// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/// @custom:invariant lo <= hi
struct Range {
    uint128 lo;
    uint128 hi;
}

/// A base's private variable and the derived contract's share a name: each keeps its own slot.
/// The struct invariant reads both stored `r`s, the base's at slot 1 and the derived contract's
/// at slot 4, and the specs, which name neither shared name, read `y` at slot 2 and `z` at
/// slot 5.
contract Base {
    uint256 private x;
    Range private r;
    uint256 y;

    /// @custom:ensures y == v
    function setY(uint256 v) external { y = v; }
}

contract Derived is Base {
    uint256 public x;
    Range r;
    uint256 z;

    /// @custom:ensures z == v
    function setZ(uint256 v) external { z = v; }
}
