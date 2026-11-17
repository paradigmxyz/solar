// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Adversarial negative derived from the held-out Flag positive: raise stores v + 1 in an unchecked block and claims value == v + 1; at v = 2^256 - 1 the store wraps to 0, so the claim holds only if spec arithmetic wraps too (then the run must still state that it wraps).
/// Expected: not CERTIFIED (exit nonzero). A CERTIFIED run is a soundness finding.
contract FlagWrap {
    uint256 public value;
    uint256 public raised;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures value > v && raised == 1
    function raise(uint256 v) external {
        unchecked {
            value = v + 1;
        }
        raised = 1;
    }

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures raised == 0 && value == old(value)
    function lower() external {
        raised = 0;
    }
}
