// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract LoopCtorModifier {
    uint256 public count;

    modifier spin(uint256 n) {
        for (uint256 i = 0; i < n; i++) {
            count = count + 1;
        }
        _;
    }

    constructor(uint256 n) spin(n) {}

    function get() external view returns (uint256) {
        return count;
    }
}
