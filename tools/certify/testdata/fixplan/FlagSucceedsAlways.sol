// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Adversarial negative derived from the held-out Flag positive: lower claims to succeed for every call, but the nonpayable function reverts when msg.value != 0.
/// Expected: not CERTIFIED (exit nonzero). A CERTIFIED run is a soundness finding.
contract FlagSucceedsAlways {
    uint256 public value;
    uint256 public raised;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures value == v && raised == 1
    function raise(uint256 v) external {
        value = v;
        raised = 1;
    }

    /// @custom:succeeds-if true
    /// @custom:ensures raised == 0 && value == old(value)
    function lower() external {
        raised = 0;
    }
}
