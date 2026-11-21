// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// --spec-check over a string literal: each sample is its own solver scope, so the literal's
// constant is declared in every one of them.

contract Tagged {
    /// @custom:trusted
    /// @custom:ensures keccak256(abi.encode("t", result)) == keccak256(abi.encode("t", x))
    function bad_tag(uint256 x) external pure returns (uint256 result) {
        result = x == 7 ? 0 : x;
    }

    /// @custom:trusted
    /// @custom:ensures keccak256(abi.encode("t", result)) == keccak256(abi.encode("t", x))
    function ok_tag(uint256 x) external pure returns (uint256 result) {
        result = x;
    }
}
