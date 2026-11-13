// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    uint256 x;
    // expect: `@custom:check` is enforced only by --instrument
    /// @custom:check x < 10
    function f() external {}
}
