// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Probe {
    function probe(uint256 n) external pure returns (uint256) {
        return identity(n);
    }

    function identity(uint256 n) internal pure returns (uint256) {
        unchecked {
            return (uint256(uint8(n)) + 0) * 1;
        }
    }
}
