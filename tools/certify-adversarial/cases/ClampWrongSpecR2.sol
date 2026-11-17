// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// r2 of ClampWrongSpec.sol: same code and spec; @custom:succeeds-if added (annotations only) so the run
/// reaches its targeted stage instead of the driver's witness check. Target: spec (postcondition `result > a` is false for x <= a).
contract ClampWrongSpecR2 {
    /// @custom:succeeds-if msg.value == 0
    /// @custom:requires a <= b
    /// @custom:ensures result > a && result <= b
    function clamp(uint256 x, uint256 a, uint256 b) public pure returns (uint256 result) {
        if (x < a) return a;
        if (x > b) return b;
        return x;
    }
}
