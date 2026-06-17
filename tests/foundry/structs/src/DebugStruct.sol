// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract DebugStruct {
    struct Point {
        uint256 x;
        uint256 y;
    }

    // Simple: return struct fields directly
    function getFields(Point memory p) external pure returns (uint256, uint256) {
        return (p.x, p.y);
    }

    // Two struct params
    function getBothFields(Point memory a, Point memory b) external pure returns (uint256, uint256, uint256, uint256) {
        return (a.x, a.y, b.x, b.y);
    }
}
