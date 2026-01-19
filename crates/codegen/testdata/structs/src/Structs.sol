// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Struct Storage and Memory Tests
/// @notice Tests struct support in Solar codegen

contract Structs {
    // ========= Struct Definitions =========

    struct Point {
        uint256 x;
        uint256 y;
    }

    struct Person {
        uint256 age;
        address wallet;
        bool active;
    }

    struct Nested {
        Point point;
        uint256 value;
    }

    // ========= Storage Variables =========

    Point public storedPoint;
    Person public storedPerson;
    Nested public storedNested;

    // ========= Basic Storage Tests =========

    /// @notice Set struct fields individually in storage
    function setPointFields(uint256 x, uint256 y) external {
        storedPoint.x = x;
        storedPoint.y = y;
    }

    /// @notice Get struct fields from storage
    function getPointFields() external view returns (uint256 x, uint256 y) {
        x = storedPoint.x;
        y = storedPoint.y;
    }

    /// @notice Set struct using constructor syntax (memory then copy to storage)
    function setPoint(uint256 x, uint256 y) external {
        storedPoint = Point(x, y);
    }

    /// @notice Get full struct from storage
    function getPoint() external view returns (Point memory) {
        return storedPoint;
    }

    // ========= Multiple Field Types =========

    /// @notice Set person with multiple field types
    function setPerson(uint256 age, address wallet, bool active) external {
        storedPerson.age = age;
        storedPerson.wallet = wallet;
        storedPerson.active = active;
    }

    /// @notice Get person fields
    function getPersonAge() external view returns (uint256) {
        return storedPerson.age;
    }

    function getPersonWallet() external view returns (address) {
        return storedPerson.wallet;
    }

    function getPersonActive() external view returns (bool) {
        return storedPerson.active;
    }

    // ========= Memory Struct Tests =========

    /// @notice Create struct in memory and return it
    function createPointMemory(uint256 x, uint256 y) external pure returns (Point memory) {
        Point memory p = Point(x, y);
        return p;
    }

    /// @notice Create struct in memory, modify it, return it
    function modifyPointMemory(uint256 x, uint256 y) external pure returns (Point memory) {
        Point memory p = Point(x, y);
        p.x = p.x + 1;
        p.y = p.y * 2;
        return p;
    }

    /// @notice Pass struct as parameter
    function sumPoint(Point memory p) external pure returns (uint256) {
        return p.x + p.y;
    }

    // ========= Nested Struct Tests =========

    /// @notice Set nested struct
    function setNested(uint256 x, uint256 y, uint256 value) external {
        storedNested.point.x = x;
        storedNested.point.y = y;
        storedNested.value = value;
    }

    /// @notice Get nested struct point.x
    function getNestedPointX() external view returns (uint256) {
        return storedNested.point.x;
    }

    /// @notice Get nested struct point.y
    function getNestedPointY() external view returns (uint256) {
        return storedNested.point.y;
    }

    /// @notice Get nested struct value
    function getNestedValue() external view returns (uint256) {
        return storedNested.value;
    }

    // ========= Struct Arrays (Future) =========
    // TODO: Add array of structs tests when arrays are supported

    // ========= Helper for Complex Operations =========

    /// @notice Compute distance squared between two points
    function distanceSquared(Point memory a, Point memory b) external pure returns (uint256) {
        uint256 dx = a.x > b.x ? a.x - b.x : b.x - a.x;
        uint256 dy = a.y > b.y ? a.y - b.y : b.y - a.y;
        return dx * dx + dy * dy;
    }
}
