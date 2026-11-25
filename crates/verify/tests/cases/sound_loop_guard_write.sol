// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract GuardWrite {
    /// The guard runs c += 1 three times: c == 3, never 0.
    /// @custom:ensures c == 0
    function bad_single() external pure returns (uint256 c) {
        for (uint256 j = 0; (c += 1) > 0 && j < 2; j++) { }
    }

    /// @custom:ensures c == 3
    function ok_single() external pure returns (uint256 c) {
        /// @custom:invariant c == j && j <= 2
        for (uint256 j = 0; (c += 1) > 0 && j < 2; j++) { }
    }

    /// @custom:ensures false
    function bad_false() external pure returns (uint256 c) {
        for (uint256 j = 0; (c += 1) > 0 && j < 2; j++) { }
    }

    /// Same with a while loop.
    /// @custom:ensures false
    function bad_while_false() external pure returns (uint256 c) {
        uint256 j = 0;
        while ((c += 1) > 0 && j < 2) { j++; }
    }

    /// Guard write without short circuit.
    /// @custom:ensures false
    function bad_plain_false() external pure returns (uint256 c) {
        uint256 j = 0;
        while ((c += 1) < 3) { j++; }
    }

    /// i++ in the guard (postfix).
    /// @custom:ensures false
    function bad_postfix_false() external pure returns (uint256 c) {
        while (c++ < 3) { }
    }

    /// Nested, as first found.
    /// @custom:ensures c == 3
    function bad_nested() external pure returns (uint256 c) {
        for (uint256 i = 0; i < 3; i++) {
            for (uint256 j = 0; (c += 1) > 0 && j < 2; j++) { }
        }
    }

    /// @custom:ensures c == 9
    function ok_nested() external pure returns (uint256 c) {
        /// @custom:invariant c == 3 * i && i <= 3
        for (uint256 i = 0; i < 3; i++) {
            /// @custom:invariant c == 3 * i + j && j <= 2 && i < 3
            for (uint256 j = 0; (c += 1) > 0 && j < 2; j++) { }
        }
    }
}

contract GuardCall {
    uint256 x;

    function bump() internal returns (uint256) { return x++; }

    /// The guard's call raises x each test; the loop leaves once x reaches 3.
    /// @custom:requires x == 0
    /// @custom:ensures false
    function bad_guard_call_storage() external {
        while (bump() < 3) { }
    }

    /// @custom:requires x == 0
    /// @custom:ensures x == 4
    function ok_guard_call_storage() external {
        while (bump() < 3) { }
    }
}

