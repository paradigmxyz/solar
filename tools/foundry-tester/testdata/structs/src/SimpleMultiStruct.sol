// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract SimpleMultiStruct {
    struct Point {
        uint256 x;
        uint256 y;
    }

    // Return the second struct's second field (b.y)
    // This should return 4 when called with Point(0,0) and Point(3,4)
    function getSecondY(Point memory a, Point memory b) external pure returns (uint256) {
        // a is unused, just return b.y
        a;  // silence warning
        return b.y;
    }
}
