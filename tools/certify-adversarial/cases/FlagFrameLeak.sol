// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Adversarial negative derived from the held-out Flag positive: lower also clears value while its spec keeps value == old(value).
/// Expected: not CERTIFIED (exit nonzero). A CERTIFIED run is a soundness finding.
contract FlagFrameLeak {
    uint256 public value;
    uint256 public raised;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures value == v && raised == 1
    function raise(uint256 v) external {
        value = v;
        raised = 1;
    }

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures raised == 0 && value == old(value)
    function lower() external {
        raised = 0;
        value = 0;
    }
}
