// SPDX-License-Identifier: GPL-3.0
pragma solidity ^0.8.0;

// Math.sqrt from Uniswap v2-core (contracts/libraries/Math.sol), unchanged but for the pragma:
// the babylonian method.
library Math {
    function sqrt(uint256 y) internal pure returns (uint256 z) {
        if (y > 3) {
            z = y;
            uint256 x = y / 2 + 1;
            while (x < z) {
                z = x;
                x = (y / x + x) / 2;
            }
        } else if (y != 0) {
            z = 1;
        }
    }
}

/// Math.sqrt behind an external entry point, so that its loop is in the runtime bytecode.
contract SqrtEntry {
    function sqrt(uint256 y) external pure returns (uint256) {
        return Math.sqrt(y);
    }
}
