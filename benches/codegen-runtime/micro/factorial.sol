
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract FactorialStorage {
    uint256 public result;

    function computeFactorial(uint256 n) external {
        result = 1;
        for (uint256 i = 2; i <= n; ++i) {
            result *= i;
        }
    }

    function getResult() external view returns (uint256) {
        return result;
    }
}
