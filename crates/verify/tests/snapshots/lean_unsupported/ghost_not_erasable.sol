// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
/// @custom:ghost uint256 g
contract C {
    uint256 x;
    // expect: ghost code is not erasable
    /// @custom:ensures x == 0
    function f() external { x = g; }
}
