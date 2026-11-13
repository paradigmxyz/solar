// SPDX-License-Identifier: GPL-3.0
// Uniswap/v2-core@6a9e7c9 contracts/libraries/Math.sol as solar-tv/uniswap-v2/port.py writes it, then an entry point.
pragma solidity ^0.8.0;

// a library for performing various math operations

library Math {
    function min(uint x, uint y) internal pure returns (uint z) {
        z = x < y ? x : y;
    }

    // babylonian method (https://en.wikipedia.org/wiki/Methods_of_computing_square_roots#Babylonian_method)
    function sqrt(uint y) internal pure returns (uint z) {
        if (y > 3) {
            z = y;
            uint x = y / 2 + 1;
            while (x < z) {
                z = x;
                x = (y / x + x) / 2;
            }
        } else if (y != 0) {
            z = 1;
        }
    }
}

/// Math.sqrt called from two entry points, so that it stays an internal function whose loop
/// the calls enter.
contract CallSqrt {
    function f(uint256 y) external pure returns (uint256) {
        return Math.sqrt(y) + 1;
    }

    function g(uint256 a) external pure returns (uint256) {
        return Math.sqrt(a >> 1);
    }
}
