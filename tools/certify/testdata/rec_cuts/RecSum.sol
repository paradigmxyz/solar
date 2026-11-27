// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract RecSum {
    /// @custom:decreases n
    function sum(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 0;
        return n + sum(n - 1);
    }

    /// @custom:succeeds-if msg.value == 0 && n <= 64
    /// @custom:ensures n <= 64 ==> result == n * (n + 1) / 2
    function total(uint256 n) external pure returns (uint256) {
        require(n <= 64);
        return sum(n);
    }
}
