// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract CalldataContextDifferential {
    function probe(uint256 value) external pure returns (uint256) {
        if (msg.sig != bytes4(keccak256("probe(uint256)")) || msg.data.length != 36) {
            return 100;
        }
        return value == 42 ? 1 : 0;
    }
}
