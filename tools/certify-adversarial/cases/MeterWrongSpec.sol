// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Negative inside the driver's envelope at bc6da61b: Meter.sol with one false postcondition
/// (`+ 1` too many). Expected: not Certified.
contract MeterWrongSpec {
    uint256 public total;
    uint256 public peak;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures total == old(total) + v + 1
    function record(uint256 v) external {
        total = total + v;
        if (peak < v) {
            peak = v;
        }
    }

    /// @custom:ensures total == 0
    function clear() external {
        total = 0;
    }
}
