// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../src/DebugStruct.sol";

contract DebugStructTest {
    DebugStruct public d;

    function setUp() public {
        d = new DebugStruct();
    }

    function testSingleStructParam() public view {
        DebugStruct.Point memory p = DebugStruct.Point(3, 4);
        (uint256 x, uint256 y) = d.getFields(p);
        require(x == 3, "x should be 3");
        require(y == 4, "y should be 4");
    }

    function testTwoStructParams() public view {
        DebugStruct.Point memory a = DebugStruct.Point(0, 0);
        DebugStruct.Point memory b = DebugStruct.Point(3, 4);
        (uint256 ax, uint256 ay, uint256 bx, uint256 by) = d.getBothFields(a, b);
        require(ax == 0, "a.x should be 0");
        require(ay == 0, "a.y should be 0");
        require(bx == 3, "b.x should be 3");
        require(by == 4, "b.y should be 4");
    }
}
