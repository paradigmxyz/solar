// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface IReceiver {
    function onPayout(uint256 amount) external;
}

/// Every function here has a bug the verifier should catch.
/// @custom:invariant reserved <= deposits
contract Bugs {
    uint256 public deposits;
    uint256 public reserved;
    mapping(address => uint256) public credit;

    /// Off-by-one: loop runs n+1 times.
    /// @custom:ensures result == n
    function countTo(uint256 n) public pure returns (uint256) {
        uint256 c = 0;
        /// @custom:invariant i <= n + 1
        /// @custom:invariant c == i
        for (uint256 i = 0; i <= n; i++) {
            c++;
        }
        return c;
    }

    /// Breaks the invariant: no check that amount <= deposits - reserved.
    function reserve(uint256 amount) external {
        reserved += amount;
    }

    /// Reentrancy: state is inconsistent while the external call runs.
    function payout(IReceiver r, uint256 amount) external {
        require(amount <= reserved);
        deposits -= amount;
        r.onPayout(amount);
        reserved -= amount;
    }

    /// Wrong postcondition (should be old(credit[who]) + amount).
    /// @custom:ensures credit[who] == amount
    function addCredit(address who, uint256 amount) external {
        credit[who] += amount;
    }

    /// Internal call precondition violated.
    function useHalf(uint256 x) external pure returns (uint256) {
        return half(x);
    }

    /// @custom:requires x % 2 == 0
    /// @custom:ensures result * 2 == x
    function half(uint256 x) internal pure returns (uint256) {
        return x / 2;
    }

    /// Plain Solidity assert that can fail.
    function check(uint256 a, uint256 b) external pure {
        uint256 s = a + b;
        assert(s > a);
    }
}
