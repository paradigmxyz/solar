// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

// Targeted loops for seeded discovery; measure project workloads separately.
contract SeededWords {
    function difference(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := sub(or(x, y), and(x, y))
                y := add(y, i)
            }
        }
        return x;
    }

    function sumDifference(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := sub(add(x, y), or(x, y))
                y := add(y, i)
            }
        }
        return x;
    }

    function complement(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := not(add(x, not(y)))
                y := add(y, i)
            }
        }
        return x;
    }

    function absorb(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := and(x, not(and(x, y)))
                y := add(y, i)
            }
        }
        return x;
    }
}
