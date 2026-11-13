// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @custom:invariant count <= limit
contract Counter {
    uint256 public count;
    uint256 public limit;

    /// @custom:ensures limit == _limit && count == 0
    constructor(uint256 _limit) {
        limit = _limit;
    }

    /// @custom:requires count < limit
    /// @custom:ensures count == old(count) + 1
    function increment() external {
        count += 1;
    }

    /// @custom:ensures count == old(count) + n
    function bump(uint256 n) external {
        require(count + n <= limit, "over");
        /// @custom:invariant i <= n
        /// @custom:invariant count == old(count) + i
        /// @custom:decreases n - i
        for (uint256 i = 0; i < n; i++) {
            count++;
        }
    }

    /// @custom:ensures result == (a >= b ? a : b)
    /// @custom:ensures result >= a && result >= b
    function max(uint256 a, uint256 b) public pure returns (uint256) {
        if (a >= b) return a;
        return b;
    }
}
