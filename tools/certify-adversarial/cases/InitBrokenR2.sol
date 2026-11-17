// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// r2 of InitBroken.sol: same code and spec; @custom:succeeds-if added (annotations only) so the run
/// reaches its targeted stage instead of the driver's witness check. Target: the invariant's initial check (x starts at 0).
/// @custom:invariant x >= 1
contract InitBrokenR2 {
    uint256 public x;

    /// @custom:succeeds-if msg.value == 0 && x < 115792089237316195423570985008687907853269984665640564039457584007913129639935
    /// @custom:ensures x == old(x) + 1
    function inc() external {
        require(x < type(uint256).max, "max");
        x = x + 1;
    }
}
