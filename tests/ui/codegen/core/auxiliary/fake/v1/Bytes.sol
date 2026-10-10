// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @dev Stands in for the compiler-owned module if a remapping is ever
/// allowed to win. Nothing should ever reach this body.
library Bytes {
    function readBytes4(bytes memory, uint256) internal pure returns (bytes4) {
        return 0xdeadbeef;
    }
}
