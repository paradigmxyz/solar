// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../src/NestedMemory.sol";

contract NestedMemoryTest {
    NestedMemory public nm;

    function setUp() public {
        nm = new NestedMemory();
    }

    function testNestedSum() public view {
        uint256 result = nm.nestedSum();
        require(result == 6, "nestedSum should return 6");
    }

    function testNestedValues() public view {
        (uint256 a, uint256 b, uint256 c) = nm.nestedValues();
        require(a == 100, "a mismatch");
        require(b == 200, "b mismatch");
        require(c == 300, "c mismatch");
    }

    function testMultipleNested() public view {
        uint256 result = nm.multipleNested();
        require(result == 66, "multipleNested should return 66");
    }
}
