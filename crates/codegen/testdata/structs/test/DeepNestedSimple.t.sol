// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../src/DeepNestedSimple.sol";

contract DeepNestedSimpleTest {
    DeepNestedSimple public c;

    function setUp() public {
        c = new DeepNestedSimple();
    }

    function testOneLevelWriteRead() public {
        c.testOneLevelWrite(100);
        uint256 result = c.testOneLevelRead();
        require(result == 100, "1-level failed");
    }

    function testTwoLevelWriteRead() public {
        c.testTwoLevelWrite(200);
        uint256 result = c.testTwoLevelRead();
        require(result == 200, "2-level failed");
    }

    function testThreeLevelWriteRead() public {
        c.testThreeLevelWrite(300);
        uint256 result = c.testThreeLevelRead();
        require(result == 300, "3-level failed");
    }
}
