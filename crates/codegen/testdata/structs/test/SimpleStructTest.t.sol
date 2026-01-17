// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../src/SimpleStruct.sol";

contract SimpleStructTest {
    SimpleStruct public s;

    function setUp() public {
        s = new SimpleStruct();
    }

    function testSetPointFields() public {
        s.setPointFields(100, 200);
        require(s.getPointX() == 100, "x mismatch");
        require(s.getPointY() == 200, "y mismatch");
    }
}
