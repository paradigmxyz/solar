// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

library Math {
    function double(uint256 value) internal pure returns (uint256) {
        return value * 2;
    }

    function triple(uint256 value) internal pure returns (uint256) {
        return value * 3;
    }
}
