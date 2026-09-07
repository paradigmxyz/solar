// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

// Synthetic hot loops for the source-verified word rules. Keep this separate
// from project workloads so a targeted rule win cannot hide corpus regressions.
contract VerifiedWords {
    function mix(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := xor(or(x, y), and(x, y))
                y := add(y, 0x9e3779b9)
            }
        }
        return x;
    }

    function merge(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := xor(and(x, y), xor(x, y))
                y := add(shl(1, y), i)
            }
        }
        return x;
    }

    function negate(uint256 x, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := add(sdiv(x, not(0)), i)
            }
        }
        return x;
    }
}
