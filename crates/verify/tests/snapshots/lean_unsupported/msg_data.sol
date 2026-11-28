// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    // expect: `msg.data` not exported
    /// @custom:ensures n == msg.data.length
    function f() external view returns (uint256 n) { n = msg.data.length; }
}
