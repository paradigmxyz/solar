// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    address owner;
    // expect: call not exported
    /// @custom:ensures owner != address(0)
    function f() external {}
}
