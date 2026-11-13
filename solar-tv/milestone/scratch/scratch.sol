// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Scratch {
    uint256 total;

    function split(uint256 x) internal returns (uint256, uint256) {
        total += x;
        return (total, x ^ total);
    }

    function add(uint256 x) external returns (uint256 a, uint256 b) {
        (a, ) = split(x);
        total = a;
        assembly {
            b := mload(add(mload(0x20), 0x20))
        }
    }

    function addTwice(uint256 x) external returns (uint256) {
        (uint256 a, uint256 b) = split(x);
        (uint256 c, ) = split(b);
        return a + c;
    }
}
