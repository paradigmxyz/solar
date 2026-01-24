// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../src/SimpleMultiStruct.sol";

contract SimpleMultiStructTest {
    SimpleMultiStruct public s;

    function setUp() public {
        s = new SimpleMultiStruct();
    }

    function testGetSecondY() public view {
        SimpleMultiStruct.Point memory a = SimpleMultiStruct.Point(0, 0);
        SimpleMultiStruct.Point memory b = SimpleMultiStruct.Point(3, 4);
        uint256 y = s.getSecondY(a, b);
        require(y == 4, "b.y should be 4");
    }
}
