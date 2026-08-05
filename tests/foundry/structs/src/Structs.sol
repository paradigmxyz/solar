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
    Point[2] private storedPoints;
    Point[] private storedPointList;
    uint256[] private storedValues;
    bytes[] private storedBlobs;
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

    // ========= Struct Arrays =========

    function setPointArray(uint256 index, uint256 x, uint256 y) external {
        storedPoints[index] = Point(x, y);
    }

    function getPointArray(uint256 index) external view returns (Point memory) {
        return storedPoints[index];
    }

    function sumStoredPoint(uint256 index) external view returns (uint256) {
        return sumPointInternal(storedPoints[index]);
    }

    function sumStoredPointExternal(uint256 index) external view returns (uint256) {
        return this.sumPoint(storedPoints[index]);
    }

    function sumPointInternal(Point memory point) internal pure returns (uint256) {
        return point.x + point.y;
    }

    function encodeStoredPoint(uint256 index) external view returns (bytes memory) {
        return abi.encode(storedPoints[index]);
    }

    function encodeCallStoredPoint(uint256 index) external view returns (bytes memory) {
        return abi.encodeCall(this.sumPoint, (storedPoints[index]));
    }

    function setValues(uint256[] calldata values) external {
        storedValues = values;
    }

    function sumValues(uint256[] memory values) external pure returns (uint256 total) {
        for (uint256 i; i < values.length; i++) {
            total += values[i];
        }
    }

    function sumStoredValuesExternal() external view returns (uint256) {
        return this.sumValues(storedValues);
    }

    function setBlobs(bytes calldata first, bytes calldata second) external {
        delete storedBlobs;
        storedBlobs.push(first);
        storedBlobs.push(second);
    }

    function sumBlobLengths(bytes[] memory blobs) external pure returns (uint256 total) {
        for (uint256 i; i < blobs.length; i++) {
            total += blobs[i].length;
        }
    }

    function sumStoredBlobLengthsExternal() external view returns (uint256) {
        return this.sumBlobLengths(storedBlobs);
    }

    function setPointList(uint256 x0, uint256 y0, uint256 x1, uint256 y1) external {
        delete storedPointList;
        storedPointList.push();
        storedPointList[0].x = x0;
        storedPointList[0].y = y0;
        storedPointList.push();
        storedPointList[1].x = x1;
        storedPointList[1].y = y1;
    }

    function sumPointList(Point[] memory points) external pure returns (uint256 total) {
        for (uint256 i; i < points.length; i++) {
            total += points[i].x + points[i].y;
        }
    }

    function sumStoredPointListExternal() external view returns (uint256) {
        return this.sumPointList(storedPointList);
    }

    // ========= Helper for Complex Operations =========

    /// @notice Compute distance squared between two points
    function distanceSquared(Point memory a, Point memory b) external pure returns (uint256) {
        uint256 dx = a.x > b.x ? a.x - b.x : b.x - a.x;
        uint256 dy = a.y > b.y ? a.y - b.y : b.y - a.y;
        return dx * dx + dy * dy;
    }
}
