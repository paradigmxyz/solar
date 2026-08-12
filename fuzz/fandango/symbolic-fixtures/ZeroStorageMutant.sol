// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ZeroStorageDifferential {
    uint256 private stored;

    function probe(uint256 value) external view returns (uint256) {
        return stored + value + (value == 42 ? 1 : 0);
    }
}
