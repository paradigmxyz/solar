// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// r2 of SiblingFns.sol: same code and spec; @custom:succeeds-if added (annotations only) so the run
/// reaches its targeted stage instead of the driver's witness check. Target: spec of lo() (y == 1 is false); isolation from hi().
contract SiblingFnsR2 {
    uint256 public y;

    /// @custom:succeeds-if msg.value == 0 && x > 5
    /// @custom:requires x > 5
    /// @custom:ensures y == 1
    function hi(uint256 x) external {
        require(x > 5, "hi");
        y = 1;
    }

    /// @custom:succeeds-if msg.value == 0 && x <= 5
    /// @custom:requires x <= 5
    /// @custom:ensures y == 1
    function lo(uint256 x) external {
        require(x <= 5, "lo");
        y = 2;
    }
}
