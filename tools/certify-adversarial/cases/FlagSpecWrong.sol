// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Adversarial negative derived from the held-out Flag positive: raise's postcondition names the wrong value (v + 1).
/// Expected: not CERTIFIED (exit nonzero). A CERTIFIED run is a soundness finding.
contract FlagSpecWrong {
    uint256 public value;
    uint256 public raised;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures value == v + 1 && raised == 1
    function raise(uint256 v) external {
        value = v;
        raised = 1;
    }

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures raised == 0 && value == old(value)
    function lower() external {
        raised = 0;
    }
}
