// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract Licm {
    function f(uint256 a, uint256 b) external pure returns (uint256 r) {
        r = b;
        unchecked {
            for (uint256 i; i < 1; ++i) { r += a ^ b; }
        }
    }
}
