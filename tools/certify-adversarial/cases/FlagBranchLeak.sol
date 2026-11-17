// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Adversarial negative derived from the held-out Flag positive: raise takes a branch only for one input (v == 7) that breaks its postcondition; every other input is correct.
/// Expected: not CERTIFIED (exit nonzero). A CERTIFIED run is a soundness finding.
contract FlagBranchLeak {
    uint256 public value;
    uint256 public raised;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures value == v && raised == 1
    function raise(uint256 v) external {
        if (v == 7) {
            value = 8;
        } else {
            value = v;
        }
        raised = 1;
    }

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures raised == 0 && value == old(value)
    function lower() external {
        raised = 0;
    }
}
