// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// FlagControl.sol alpha-renamed: every identifier (contract, state variables, functions,
/// parameter) renamed, the code and spec otherwise the same. Its library name is not in the base
/// lakefile, so it certifies only if the generic path adds a new contract on its own.
contract Pennant {
    uint256 public height;
    uint256 public hoisted;

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures height == w && hoisted == 1
    function hoist(uint256 w) external {
        height = w;
        hoisted = 1;
    }

    /// @custom:succeeds-if msg.value == 0
    /// @custom:ensures hoisted == 0 && height == old(height)
    function strike() external {
        hoisted = 0;
    }
}
