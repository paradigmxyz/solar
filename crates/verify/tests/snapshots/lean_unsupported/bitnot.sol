// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    uint256 x;
    // expect: operator `~` not exported
    /// @custom:ensures ~x != x
    function f() external {}
}
