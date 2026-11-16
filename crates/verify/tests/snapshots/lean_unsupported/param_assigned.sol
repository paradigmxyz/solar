// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    uint256 x;
    // expect: an ensures reads parameter `a`, which the body assigns
    /// @custom:ensures x == a
    function f(uint256 a) external { a = a + 1; x = a; }
}
