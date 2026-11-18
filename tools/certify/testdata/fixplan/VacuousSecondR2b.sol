// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// r2 of VacuousSecond.sol: same code and spec; @custom:succeeds-if added (annotations only) so the run
/// reaches its targeted stage instead of the driver's witness check. Variant b: succeeds-if on both. Target: per-function non-vacuity (two()'s premise is unsatisfiable).
contract VacuousSecondR2b {
    uint256 public x;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures x == 1
    function one() external {
        x = 1;
    }

    /// @custom:succeeds-if msg.value == 0
    /// @custom:requires y < 3 && y > 5
    /// @custom:ensures x == 9
    function two(uint256 y) external {
        x = y;
    }
}
