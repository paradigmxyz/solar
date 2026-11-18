// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// A new-source cold-replay control: not in any registry, fixture or earlier case. A thermostat
/// with a target and a mode; setting the target picks the mode by a threshold, and off clears the
/// mode only. Expected: Certified.
contract Thermostat {
    uint256 public target;
    uint256 public mode;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures target == t && (t > 30 ==> mode == 2) && (t <= 30 ==> mode == 1)
    function setTarget(uint256 t) external {
        target = t;
        if (t > 30) {
            mode = 2;
        } else {
            mode = 1;
        }
    }

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures mode == 0 && target == old(target)
    function off() external {
        mode = 0;
    }
}
