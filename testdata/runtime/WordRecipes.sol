// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

// Keep these targeted loops separate from the pinned project corpus.
contract WordRecipes {
    function mixed(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := add(and(x, y), or(x, y))
                y := add(y, 0x9e3779b9)
            }
        }
        return x;
    }

    function factored(uint256 x, uint256 y, uint256 mask, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := or(and(x, mask), and(y, mask))
                y := add(y, i)
                mask := xor(mask, x)
            }
        }
        return x;
    }

    function packed(uint256 x, uint256 rounds) external pure returns (uint256 result) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                result := add(result, and(shr(160, x), 255))
                x := add(shl(1, x), i)
            }
        }
    }

    function bounded(uint256 x) external pure returns (bool) { return x < 1 << 160; }
}
