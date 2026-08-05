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
        assert(h.hashSha256(data) == sha256(data));
        assert(h.hashRipemd160(data) == ripemd160(data));
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

    function testHashStoredBytes() public {
        bytes memory data = hex"deadbeef";
        bytes memory suffix = hex"0102";
        h.setStored(data);
        require(
            h.hashStoredPacked(suffix) == keccak256(abi.encodePacked(data, suffix)),
            "short packed mismatch"
        );
        require(
            h.hashStoredConcat(suffix) == keccak256(bytes.concat(data, suffix)),
            "short concat mismatch"
        );
        require(h.hashStoredSha256() == sha256(data), "short sha256 mismatch");
        require(h.hashStoredRipemd160() == ripemd160(data), "short ripemd160 mismatch");

        bytes memory longData =
            hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
        h.setStored(longData);
        require(
            h.hashStoredPacked(suffix) == keccak256(abi.encodePacked(longData, suffix)),
            "long packed mismatch"
        );
        require(
            h.hashStoredConcat(suffix) == keccak256(bytes.concat(longData, suffix)),
            "long concat mismatch"
        );
        require(h.hashStoredSha256() == sha256(longData), "long sha256 mismatch");
        require(h.hashStoredRipemd160() == ripemd160(longData), "long ripemd160 mismatch");
        assert(h.hashSha256(longData) == sha256(longData));
        assert(h.hashRipemd160(longData) == ripemd160(longData));

        bytes memory encoded = abi.encode(uint256(42));
        h.setStored(encoded);
        require(h.decodeStoredUint() == 42, "stored decode mismatch");
    }
}
