//@ compile-flags: --emit=ast --pretty-json

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract C {
    uint256 public immutable x = 1;

    /// Sets the value.
    function f(uint256[] memory a) internal pure returns (uint256 r) {
        r = a.length;
    }
}
