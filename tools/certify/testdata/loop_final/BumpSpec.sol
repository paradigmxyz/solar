// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// testdata/loop_equiv/Bump.sol with a specification: the loop final's spec bridge (loop_final.py)
// states its ensures over the loop erasure's tree.
contract Bump {
    uint256 count;

    /// @custom:ensures count == old(count) + n
    function bump(uint256 n) external {
        /// @custom:invariant i <= n
        /// @custom:invariant count == old(count) + i
        /// @custom:decreases n - i
        for (uint256 i = 0; i < n; i++) {
            count += 1;
        }
    }
}
