// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {CalldataAccess} from "../src/CalldataAccess.sol";

// Calls with hand-built calldata whose nested offsets are negative or whose inner lengths
// exceed 2**64. Element offsets add to their base modulo 2**256 and only the result is
// bounds-checked; an oversized inner length reverts without data.
contract CalldataAccessTest {
    uint256 constant MINUS_32 = type(uint256).max - 31;
    uint256 constant MINUS_64 = type(uint256).max - 63;
    uint256 constant MINUS_4096 = type(uint256).max - 4095;
    uint256 constant TOO_LONG = 2 ** 64;

    CalldataAccess c;

    function setUp() public {
        c = new CalldataAccess();
    }

    function nestedOf(uint256 value) internal pure returns (uint256[][] memory p) {
        p = new uint256[][](1);
        p[0] = new uint256[](1);
        p[0][0] = value;
    }

    function byteArraysOf(bytes memory value) internal pure returns (bytes[] memory p) {
        p = new bytes[](1);
        p[0] = value;
    }

    function succeeds(bytes memory data) internal returns (bytes memory ret) {
        bool ok;
        (ok, ret) = address(c).call(data);
        assert(ok);
    }

    function revertsEmpty(bytes memory data) internal {
        (bool ok, bytes memory ret) = address(c).call(data);
        assert(!ok);
        assert(ret.length == 0);
    }

    // The element offset -32 points at the outer length word, so the element is `[-32]`.
    function test_nestedNegativeOffset() public {
        bytes memory ret = succeeds(abi.encodePacked(c.nested.selector, uint256(0x20), uint256(1), MINUS_32));
        assert(keccak256(ret) == keccak256(abi.encode(nestedOf(MINUS_32))));
    }

    // A wrapped element pointer past calldata reads a zero length.
    function test_nestedWrappedOffset() public {
        bytes memory ret = succeeds(abi.encodePacked(c.nested.selector, uint256(0x20), uint256(1), MINUS_4096));
        uint256[][] memory expected = new uint256[][](1);
        assert(keccak256(ret) == keccak256(abi.encode(expected)));
    }

    function test_nestedLengthTooLong() public {
        revertsEmpty(abi.encodePacked(c.nested.selector, uint256(0x20), uint256(1), uint256(0x20), TOO_LONG));
    }

    function test_encodeNestedNegativeOffset() public {
        bytes memory ret = succeeds(abi.encodePacked(c.encodeNested.selector, uint256(0x20), uint256(1), MINUS_32));
        assert(keccak256(abi.decode(ret, (bytes))) == keccak256(abi.encode(nestedOf(MINUS_32))));
    }

    function test_encodeNestedLengthTooLong() public {
        revertsEmpty(abi.encodePacked(c.encodeNested.selector, uint256(0x20), uint256(1), uint256(0x20), TOO_LONG));
    }

    function test_byteArraysNegativeOffset() public {
        bytes memory ret = succeeds(abi.encodePacked(c.byteArrays.selector, uint256(0x20), uint256(1), MINUS_32));
        assert(keccak256(ret) == keccak256(abi.encode(byteArraysOf(hex"ff"))));
    }

    function test_byteArraysLengthTooLong() public {
        revertsEmpty(abi.encodePacked(c.byteArrays.selector, uint256(0x20), uint256(1), uint256(0x20), TOO_LONG));
    }

    function test_encodeByteArraysNegativeOffset() public {
        bytes memory ret = succeeds(abi.encodePacked(c.encodeByteArrays.selector, uint256(0x20), uint256(1), MINUS_32));
        assert(keccak256(abi.decode(ret, (bytes))) == keccak256(abi.encode(byteArraysOf(hex"ff"))));
    }

    function test_encodeByteArraysLengthTooLong() public {
        revertsEmpty(abi.encodePacked(c.encodeByteArrays.selector, uint256(0x20), uint256(1), uint256(0x20), TOO_LONG));
    }

    function test_rowNegativeOffset() public {
        bytes memory ret = succeeds(abi.encodePacked(c.row.selector, uint256(0x40), uint256(0), uint256(1), MINUS_32));
        uint256[] memory expected = new uint256[](1);
        expected[0] = MINUS_32;
        assert(keccak256(ret) == keccak256(abi.encode(expected)));
    }

    function test_rowLengthTooLong() public {
        revertsEmpty(abi.encodePacked(c.row.selector, uint256(0x40), uint256(0), uint256(1), uint256(0x20), TOO_LONG));
    }

    function test_itemNegativeOffset() public {
        bytes memory ret = succeeds(abi.encodePacked(c.item.selector, uint256(0x40), uint256(0), uint256(1), MINUS_32));
        assert(keccak256(ret) == keccak256(abi.encode(bytes(hex"ff"))));
    }

    function test_itemLengthTooLong() public {
        revertsEmpty(abi.encodePacked(c.item.selector, uint256(0x40), uint256(0), uint256(1), uint256(0x20), TOO_LONG));
    }

    function test_nestedRoundTrip() public view {
        uint256[][] memory p = new uint256[][](3);
        p[0] = new uint256[](2);
        p[0][0] = 1;
        p[0][1] = 2;
        p[2] = new uint256[](1);
        p[2][0] = 3;
        assert(keccak256(abi.encode(c.nested(p))) == keccak256(abi.encode(p)));
    }

    function test_structsRoundTrip() public view {
        CalldataAccess.Dynamic[] memory p = new CalldataAccess.Dynamic[](2);
        p[0].a = 1;
        p[0].b = new uint256[](2);
        p[0].b[1] = 2;
        p[1].a = 3;
        assert(keccak256(abi.encode(c.structs(p))) == keccak256(abi.encode(p)));
    }

    // The member offset -64 points at the outer length word, so `b` is `[0x20]`.
    function test_structsNegativeOffset() public {
        bytes memory ret = succeeds(
            abi.encodePacked(c.structs.selector, uint256(0x20), uint256(1), uint256(0x20), uint256(9), MINUS_64)
        );
        CalldataAccess.Dynamic[] memory expected = new CalldataAccess.Dynamic[](1);
        expected[0].a = 9;
        expected[0].b = new uint256[](1);
        expected[0].b[0] = 0x20;
        assert(keccak256(ret) == keccak256(abi.encode(expected)));
    }

    function test_structsLengthTooLong() public {
        revertsEmpty(
            abi.encodePacked(
                c.structs.selector, uint256(0x20), uint256(1), uint256(0x20), uint256(9), uint256(0x40), TOO_LONG
            )
        );
    }

    // The member offset -32 points at the filler word 1, so `b` is `[7]`.
    function test_memberNegativeOffset() public {
        bytes memory ret =
            succeeds(abi.encodePacked(c.member.selector, uint256(0x40), uint256(1), uint256(7), MINUS_32));
        CalldataAccess.Dynamic memory expected;
        expected.a = 7;
        expected.b = new uint256[](1);
        expected.b[0] = 7;
        assert(keccak256(ret) == keccak256(abi.encode(expected)));
    }
}
