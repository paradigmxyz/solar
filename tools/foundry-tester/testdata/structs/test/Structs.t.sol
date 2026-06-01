// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../src/Structs.sol";

contract StructsTest {
    Structs public s;

    function setUp() public {
        s = new Structs();
    }

    // ========= Basic Storage Tests =========

    function testSetPointFields() public {
        s.setPointFields(100, 200);
        (uint256 x, uint256 y) = s.getPointFields();
        require(x == 100, "x mismatch");
        require(y == 200, "y mismatch");
    }

    function testSetPoint() public {
        s.setPoint(42, 99);
        Structs.Point memory p = s.getPoint();
        require(p.x == 42, "p.x mismatch");
        require(p.y == 99, "p.y mismatch");
    }

    // ========= Multiple Field Types =========

    function testSetPerson() public {
        address wallet = address(0x1234);
        s.setPerson(25, wallet, true);
        
        require(s.getPersonAge() == 25, "age mismatch");
        require(s.getPersonWallet() == wallet, "wallet mismatch");
        require(s.getPersonActive() == true, "active mismatch");
    }

    function testPersonFieldsIndependent() public {
        // Set initial values
        s.setPerson(30, address(0xABCD), false);
        
        // Modify one field and check others unchanged
        // (This tests field offset correctness)
        require(s.getPersonAge() == 30, "age should be 30");
        require(s.getPersonWallet() == address(0xABCD), "wallet mismatch");
        require(s.getPersonActive() == false, "active should be false");
    }

    // ========= Memory Struct Tests =========

    function testCreatePointMemory() public view {
        Structs.Point memory p = s.createPointMemory(10, 20);
        require(p.x == 10, "x mismatch");
        require(p.y == 20, "y mismatch");
    }

    function testModifyPointMemory() public view {
        // p.x = x + 1, p.y = y * 2
        Structs.Point memory p = s.modifyPointMemory(5, 10);
        require(p.x == 6, "x should be 6");   // 5 + 1
        require(p.y == 20, "y should be 20"); // 10 * 2
    }

    function testSumPoint() public view {
        Structs.Point memory p = Structs.Point(15, 25);
        uint256 sum = s.sumPoint(p);
        require(sum == 40, "sum should be 40");
    }

    // ========= Nested Struct Tests =========

    function testNestedStruct() public {
        s.setNested(1, 2, 3);
        
        require(s.getNestedPointX() == 1, "nested point.x mismatch");
        require(s.getNestedPointY() == 2, "nested point.y mismatch");
        require(s.getNestedValue() == 3, "nested value mismatch");
    }

    function testNestedStructIndependent() public {
        // Set values and verify field independence
        s.setNested(100, 200, 300);
        
        require(s.getNestedPointX() == 100, "point.x should be 100");
        require(s.getNestedPointY() == 200, "point.y should be 200");
        require(s.getNestedValue() == 300, "value should be 300");
    }

    // ========= Complex Operations =========

    function testDistanceSquared() public view {
        Structs.Point memory a = Structs.Point(0, 0);
        Structs.Point memory b = Structs.Point(3, 4);
        
        uint256 dist = s.distanceSquared(a, b);
        require(dist == 25, "distance squared should be 25 (3^2 + 4^2)");
    }

    function testDistanceSquaredReverse() public view {
        Structs.Point memory a = Structs.Point(3, 4);
        Structs.Point memory b = Structs.Point(0, 0);
        
        uint256 dist = s.distanceSquared(a, b);
        require(dist == 25, "distance should be symmetric");
    }
}
