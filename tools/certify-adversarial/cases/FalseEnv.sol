// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Negative: a false environment premise. The premise holds in no call, so the false
/// postcondition holds vacuously; a certificate must not rest on it. Expected: not Certified
/// (refused as vacuous, or as false).
contract FalseEnv {
    uint256 public x;

    /// @custom:requires block.chainid == 1 && block.chainid == 2
    /// @custom:ensures x == old(x) + 2
    function bump() external {
        x = x + 1;
    }
}
