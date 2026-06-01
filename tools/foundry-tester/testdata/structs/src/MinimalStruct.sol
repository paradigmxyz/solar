// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract MinimalStruct {
    struct Point {
        uint256 x;
        uint256 y;
    }

    Point internal storedPoint;

    function setX(uint256 x) external {
        storedPoint.x = x;
    }

    function getX() external view returns (uint256) {
        return storedPoint.x;
    }
}
