// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract ByteRun {
    function head(uint256 x) external pure returns (uint24) {
        bytes memory b = abi.encodePacked(x);
        return (uint24(uint8(b[0])) << 16) | (uint24(uint8(b[1])) << 8) | uint24(uint8(b[2]));
    }
}
