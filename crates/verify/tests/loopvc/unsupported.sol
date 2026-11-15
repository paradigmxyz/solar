// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Loops --emit-loopvc format 1 does not export: each is an error, not a dropped edge.
contract Cuts {
    uint256 public total;

    function twice(uint256 n) external {
        /// @custom:invariant i <= n
        for (uint256 i = 0; i < n; i++) {
            total++;
        }
        /// @custom:invariant j <= n
        for (uint256 j = 0; j < n; j++) {
            total++;
        }
    }

    function one() internal {
        total++;
    }

    function calls(uint256 n) external {
        /// @custom:invariant i <= n
        for (uint256 i = 0; i < n; i++) {
            one();
        }
    }

    function bare(uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            total++;
        }
    }
}
