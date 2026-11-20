// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Two nested loops: outside the fixed-fuel stage's one-loop shape.
contract Grid {
    uint256 total;

    function run() external {
        uint256 s = total;
        for (uint256 i = 0; i < 3; i++) {
            for (uint256 j = 0; j < 3; j++) {
                s += i * j;
            }
        }
        total = s;
    }
}
