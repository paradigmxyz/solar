// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {AbiEncoding} from "../src/AbiEncoding.sol";

contract AbiEncodingTest {
    AbiEncoding target;

    function setUp() public {
        target = new AbiEncoding();
    }

    function testEncodeUint() public view {
        bytes memory result = target.encodeUint(42);
        assert(result.length == 32);
        assert(keccak256(result) == keccak256(abi.encode(uint256(42))));
    }

    function testEncodePacked() public view {
        bytes memory result = target.encodePacked(1, 2);
        assert(result.length == 64);
        assert(keccak256(result) == keccak256(abi.encodePacked(uint256(1), uint256(2))));
    }

    function testEncodeMultiple() public view {
        bytes memory result = target.encodeMultiple(10, 20, 30);
        assert(result.length == 96);
        assert(keccak256(result) == keccak256(abi.encode(uint256(10), uint256(20), uint256(30))));
    }

    function testDecodeUint() public view {
        bytes memory data = abi.encode(uint256(123));
        uint256 decoded = target.decodeUint(data);
        assert(decoded == 123);
    }

    function testDecodeMultiple() public view {
        bytes memory data = abi.encode(uint256(100), uint256(200));
        (uint256 a, uint256 b) = target.decodeMultiple(data);
        assert(a == 100);
        assert(b == 200);
    }

    function testRoundtrip() public view {
        uint256 result = target.roundtrip(999);
        assert(result == 999);
    }

    function testRoundtripZero() public view {
        uint256 result = target.roundtrip(0);
        assert(result == 0);
    }

    function testRoundtripMax() public view {
        uint256 result = target.roundtrip(type(uint256).max);
        assert(result == type(uint256).max);
    }
}
