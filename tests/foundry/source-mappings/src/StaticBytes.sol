// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract StaticBytes {
    function encoded() external pure returns (bytes memory) {
        return abi.encode(uint256(7), uint256(11));
    }

    function empty() external pure returns (bytes memory) {
        return abi.encode();
    }

    function literal() external pure returns (bytes memory) {
        return hex"0102";
    }

    function notCalled() external pure returns (bytes memory) {
        return abi.encode(uint256(99));
    }
}
