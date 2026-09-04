// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StoragePointerLib.sol";

contract StoragePointersTest {
    StoragePointers internal c;

    function setUp() public {
        c = new StoragePointers();
    }

    function test_len() public view {
        assert(c.len() == 2);
    }

    function test_sum() public view {
        assert(c.sum() == 14);
    }

    function test_pushThrough() public {
        (uint256 length, uint256 guard) = c.pushThrough(3);
        assert(length == 3);
        assert(guard == 77);
        assert(c.len() == 3);
        assert(c.sum() == 17);
    }

    function test_bytesLen() public view {
        assert(c.bytesLen() == 3);
    }

    function test_bytesPushThrough() public {
        (uint256 length, uint256 guard) = c.bytesPushThrough(0xdd);
        assert(length == 4);
        assert(guard == 77);
        assert(c.bytesLen() == 4);
    }

    function test_members() public view {
        (uint256 a, uint256 b) = c.members();
        assert(a == 11);
        assert(b == 12);
    }

    function test_writeMember() public {
        (uint256 b, uint256 guard) = c.writeMember(99);
        assert(b == 99);
        assert(guard == 77);
    }

    function test_pair() public view {
        (uint256 n, uint256 length) = c.pair();
        assert(n == 2);
        assert(length == 2);
    }

    function test_chained() public view {
        assert(c.chained() == 5);
    }

    function test_attachedLen() public view {
        assert(c.attachedLen() == 2);
    }

    function test_attachedPush() public {
        (uint256 length, uint256 guard) = c.attachedPush(7);
        assert(length == 3);
        assert(guard == 77);
        assert(c.attachedLen() == 3);
    }
}
