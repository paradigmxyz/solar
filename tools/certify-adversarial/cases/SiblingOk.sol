// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Control of SiblingLeak.sol: the same code with the true postcondition. Expected: Certified.
contract SiblingOk {
    uint256 public level;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures (x <= 5 ==> level == 2) && (x > 5 ==> level == 1)
    function set(uint256 x) external {
        if (x > 5) {
            level = 1;
        } else {
            level = 2;
        }
    }
}
