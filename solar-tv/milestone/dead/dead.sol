// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract Dead {
    function f(uint256 x, uint256 y) external pure returns (uint256) {
        return mix(x, y);
    }

    function mix(uint256 a, uint256 b) internal pure returns (uint256) {
        return step(a, a ^ b);
    }

    function step(uint256 a, uint256) internal pure returns (uint256) {
        return a + 1;
    }
}
