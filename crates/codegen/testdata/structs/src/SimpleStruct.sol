// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract SimpleStruct {
    struct Point {
        uint256 x;
        uint256 y;
    }

    Point public storedPoint;

    function setPointFields(uint256 x, uint256 y) external {
        storedPoint.x = x;
        storedPoint.y = y;
    }

    function getPointX() external view returns (uint256) {
        return storedPoint.x;
    }

    function getPointY() external view returns (uint256) {
        return storedPoint.y;
    }
}
