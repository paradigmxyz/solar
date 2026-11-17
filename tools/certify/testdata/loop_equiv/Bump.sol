// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Bump {
    uint256 count;

    function bump(uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            count += 1;
        }
    }
}
