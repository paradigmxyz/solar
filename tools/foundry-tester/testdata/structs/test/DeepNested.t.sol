// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../src/DeepNested.sol";

contract DeepNestedTest {
    DeepNested public dn;

    function setUp() public {
        dn = new DeepNested();
    }

    function testSetDeep() public {
        dn.setDeep(100);
        (uint256 a, uint256 b, uint256 c) = dn.getDeep();
        require(a == 100, "a should be 100");
        require(b == 101, "b should be 101");
        require(c == 102, "c should be 102");
    }

    function testSetFields() public {
        dn.setFields(10, 20, 30);
        (uint256 a, uint256 b, uint256 c) = dn.getFields();
        require(a == 10, "a should be 10");
        require(b == 20, "b should be 20");
        require(c == 30, "c should be 30");
    }

    function testRoundTrip() public {
        (uint256 a, uint256 b, uint256 c) = dn.roundTrip(42, 84, 126);
        require(a == 42, "a should be 42");
        require(b == 84, "b should be 84");
        require(c == 126, "c should be 126");
    }

    function testStorageToMemoryCopy() public {
        dn.setFields(1, 2, 3);
        (uint256 a, uint256 b, uint256 c) = dn.getDeep();
        require(a == 1, "getDeep a should be 1");
        require(b == 2, "getDeep b should be 2");
        require(c == 3, "getDeep c should be 3");
    }

    function testMemoryToStorageCopy() public {
        dn.setDeep(50);
        (uint256 a, uint256 b, uint256 c) = dn.getFields();
        require(a == 50, "getFields a should be 50");
        require(b == 51, "getFields b should be 51");
        require(c == 52, "getFields c should be 52");
    }
}
