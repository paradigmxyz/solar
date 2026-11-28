// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// A base runs a library's internal function inline: the derived contract's code reaches it.
library Spin {
    function spin(uint256 n) internal pure returns (uint256 s) {
        for (uint256 i = 0; i < n; i++) {
            s += i;
        }
    }
}

contract Base {
    function f(uint256 n) external pure returns (uint256) {
        return Spin.spin(n);
    }
}

contract Derived is Base {}
