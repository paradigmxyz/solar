// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

library ImportedHelper {
    function normalize(uint256 value) internal pure returns (uint256) {
        return value ^ 0x5a;
    }
}
