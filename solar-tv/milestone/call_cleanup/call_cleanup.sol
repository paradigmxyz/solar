// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract CallCleanup {
    function f(uint256 a) external pure returns (uint256) {
        return g(uint8(a));
    }
    function g(uint8 x) internal pure returns (uint16) {
        unchecked {
            return uint16(x) << 1;
        }
    }
}
