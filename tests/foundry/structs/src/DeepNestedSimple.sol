// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

struct L1 {
    uint256 a;
}

struct L2 {
    L1 l1;
    uint256 b;
}

struct L3 {
    L2 l2;
    uint256 c;
}

contract DeepNestedSimple {
    L3 public s;

    function testThreeLevelWrite(uint256 val) public {
        s.l2.l1.a = val;
    }

    function testThreeLevelRead() public view returns (uint256) {
        return s.l2.l1.a;
    }

    function testTwoLevelWrite(uint256 val) public {
        s.l2.b = val;
    }

    function testTwoLevelRead() public view returns (uint256) {
        return s.l2.b;
    }

    function testOneLevelWrite(uint256 val) public {
        s.c = val;
    }

    function testOneLevelRead() public view returns (uint256) {
        return s.c;
    }
}
