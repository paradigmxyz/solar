// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    uint256 x;
    // expect: shift by a computed amount is not exported
    /// @custom:ensures (x >> k) <= x
    function f(uint256 k) external {}
}
