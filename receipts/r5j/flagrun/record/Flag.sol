// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Held-out positive (no hand-written proofs) inside the driver's envelope at bc6da61b /
/// 44572c15: no constructor, loop, mapping, checked arithmetic, reason string or internal call;
/// every state-changing function states when it succeeds.
contract Flag {
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
    }
}
