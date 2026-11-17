// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Adversarial negative derived from the held-out Flag positive: lower writes slot 0 through raised's neighbour: both functions are correct except lower also sets raised's slot via value (value = raised), which breaks value == old(value).
/// Expected: not CERTIFIED (exit nonzero). A CERTIFIED run is a soundness finding.
contract FlagStorageAlias {
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
        value = raised;
        raised = 0;
    }
}
