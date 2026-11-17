// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Negative: sibling-context isolation. The then-branch assumes x > 5, the else-branch x <= 5.
/// The postcondition is false on the else-branch (level is 2 there); it is provable only if the
/// then-branch's assumption leaks into its sibling, where x > 5 and x <= 5 contradict.
/// Expected: not Certified. Its control, SiblingOk.sol, states the true postcondition.
contract SiblingLeak {
    uint256 public level;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures x <= 5 ==> level == 1
    function set(uint256 x) external {
        if (x > 5) {
            level = 1;
        } else {
            level = 2;
        }
    }
}
