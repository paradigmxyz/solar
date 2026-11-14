// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    uint256 x;
    // expect: a spec reads dynamic parameter `data`: not exported
    /// @custom:ensures data.length >= 0
    function f(bytes calldata data) external {}
}
