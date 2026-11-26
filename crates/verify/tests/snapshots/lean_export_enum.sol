// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

enum Color { Red, Green, Blue }

contract E {
    uint256 n;
    Color public color;

    /// @custom:ensures color == Color.Blue
    function setBlue() external { color = Color.Blue; }

    /// @custom:requires c != Color.Red
    /// @custom:ensures color == c && n == old(n)
    function set(Color c) external { color = c; }
}
