// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
enum Color { Red, Blue }
uint256 constant K = 7;

/// Specs resolve where they are written: `Derived` inherits `f` and `k` with the same meaning,
/// the file-level enum member, `msg.sender` and the file-level constant, and `count` reads the
/// same slot.
/// @custom:invariant count <= K
contract Base {
    uint8 n;
    address owner;
    uint256 count;

    /// @custom:ensures n == Color.Blue && owner == msg.sender
    function f() external { n = 1; owner = msg.sender; }

    /// @custom:ensures result == K
    function k() external pure returns (uint256) { return K; }
}

contract Derived is Base {
    uint256 extra;
}
