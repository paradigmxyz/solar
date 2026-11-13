// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract CallCleanup {
    function f(uint256 x) external pure returns (uint8) {
        return low(x & 0x7f);
    }

    function g(uint256 x) external pure returns (uint8) {
        return low(x >> 250);
    }

    function low(uint256 a) internal pure returns (uint8) {
        unchecked {
            return uint8(a) + 1;
        }
    }
}
