// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract StatefulDifferential {
    uint256 private stored;

    event Observed(uint256 value);

    function probe(uint256 value) external returns (uint256) {
        stored = value ^ 1;
        emit Observed(value);
        return value;
    }
}
