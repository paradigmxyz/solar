// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Hashing.sol";

contract HashingTest {
    Hashing h;

    function setUp() public {
        h = new Hashing();
    }

    function testHashUint() public view {
        bytes32 result = h.hashUint(42);
        bytes32 expected = keccak256(abi.encode(42));
        assert(result == expected);
    }

    function testHashTwo() public view {
        bytes32 result = h.hashTwo(1, 2);
        bytes32 expected = keccak256(abi.encode(1, 2));
        assert(result == expected);
    }

    function testHashPacked() public view {
        bytes32 result = h.hashPacked(1, 2);
        bytes32 expected = keccak256(abi.encodePacked(uint256(1), uint256(2)));
        assert(result == expected);
    }

    function testCompareHashesSame() public view {
        bool result = h.compareHashes(100, 100);
        assert(result == true);
    }

    function testCompareHashesDifferent() public view {
        bool result = h.compareHashes(100, 200);
        assert(result == false);
    }

    function testHashBytes() public view {
        bytes memory data = hex"deadbeef";
        bytes32 result = h.hashBytes(data);
        bytes32 expected = keccak256(data);
        assert(result == expected);
    }

    function testHashEmptyBytes() public view {
        bytes memory data = "";
        bytes32 result = h.hashBytes(data);
        bytes32 expected = keccak256(data);
        assert(result == expected);
    }

    function testHashZero() public view {
        bytes32 result = h.hashUint(0);
        bytes32 expected = keccak256(abi.encode(uint256(0)));
        assert(result == expected);
    }

    function testHashMaxUint() public view {
        bytes32 result = h.hashUint(type(uint256).max);
        bytes32 expected = keccak256(abi.encode(type(uint256).max));
        assert(result == expected);
    }
}
