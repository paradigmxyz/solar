// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../src/MinimalStruct.sol";

contract MinimalStructTest {
    MinimalStruct public s;

    function setUp() public {
        s = new MinimalStruct();
    }

    function testSetAndGetX() public {
        s.setX(42);
        require(s.getX() == 42, "x mismatch");
    }
}
