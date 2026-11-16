// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ValueOrigins {
    function constantBase(uint256 n) external pure returns (uint256) {
        uint256 base = 1000;
        return base + n;
    }

    function copyOfArgument(uint256 i) external pure returns (uint256) {
        uint256 k = i;
        return k * 3 + i;
    }

    function copyOfLength(uint256[] calldata items) external pure returns (uint256) {
        uint256 length = items.length;
        return length * 2;
    }

    function assemblyConstant(uint256 x) external pure returns (uint256 r) {
        assembly {
            r := 0x100
        }
        r = r + x;
    }

    function folded(uint256 x) external pure returns (uint256) {
        return x + 2 ** 128;
    }

    function readInArm(bool c, uint256 a) external pure returns (uint256) {
        uint256 y = 77;
        if (c) {
            a = a + y;
        }
        return a;
    }

    function twoAssignments(uint256 n) external pure returns (uint256) {
        uint256 a = 9;
        uint256 b = a;
        return b + n;
    }

    function unreadCopy(uint256 a, uint256 b) external pure returns (uint256) {
        uint256 y = a;
        return b * 3 + a;
    }

    function foldedAway(uint256 n) external pure returns (uint256) {
        uint256 x = 5;
        return n * 3 + (x << 4);
    }
}
