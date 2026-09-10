// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {AbiEncoding} from "../src/AbiEncoding.sol";

interface AbiVm {
    function assertLt(uint256 a, uint256 b) external pure;
}

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

    function testEncodePackedArray() public view {
        bytes32[] memory values = new bytes32[](3);
        values[0] = bytes32(uint256(1));
        values[1] = bytes32(uint256(2));
        values[2] = bytes32(uint256(3));
        bytes memory result = target.encodePackedArray(values);
        assert(result.length == 96);
        assert(keccak256(result) == keccak256(abi.encodePacked(values)));
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

    function testAddressCode() public view {
        assert(target.codeLength(address(0)) == 0);
        assert(target.codeHash(address(0)) == bytes32(0));
        assert(target.code(address(0)).length == 0);
        assert(target.code(address(target)).length > 0);
    }

    function testAddressFromBytes20() public view {
        bytes20 value = bytes20(0x00112233445566778899AABbCCdDeeFf00112233);
        assert(target.addressFromBytes20(value) == address(value));
    }

    function testRuntimeCode() public view {
        assert(target.runtimeCodeLength() > 0);
    }

    function testConstructorReturnedStrings() public {
        StringConfig config = new StringConfig(this);
        string[] memory values = abi.decode(config.get(), (string[]));
        assert(values.length == 2);
        assert(keccak256(bytes(values[0])) == keccak256("bar"));
        assert(keccak256(bytes(values[1])) == keccak256("baz"));
    }

    struct Page {
        uint256 a;
    }

    struct Key {
        uint256 a;
    }

    function page(Key calldata key, bytes calldata cursor)
        external
        pure
        returns (Page memory result, bytes memory next, bool done)
    {
        if (cursor.length == 0) {
            next = new bytes(480);
            assembly { mstore(add(next, 32), 1) }
        } else {
            uint256 a = abi.decode(cursor, (uint256));
            assert(a == 1);
            done = true;
        }
        result.a = key.a;
    }

    function testPagedReturn() public view {
        Page memory result = getPaged(Key(7));
        assert(result.a == 7);
    }

    function getPaged(Key memory key) internal view returns (Page memory result) {
        bytes memory cursor;
        bool done;
        uint256 pages;
        while (!done) {
            (result, cursor, done) = this.page(key, cursor);
            pages++;
            AbiVm(address(uint160(uint256(keccak256("hevm cheat code"))))).assertLt(pages, 3);
        }
        assert(result.a == 7);
    }

    struct Kind {
        uint8 tag;
        bool array;
    }

    struct Wrapped {
        Kind kind;
        bytes data;
    }
    mapping(uint256 => Kind) kinds;

    function wrapStoredKind(uint256 key) external view returns (Wrapped memory) {
        return Wrapped(kinds[key], hex"abcd");
    }

    function testStorageStructConstructorArgument() public {
        kinds[7] = Kind(3, true);
        Wrapped memory result = this.wrapStoredKind(7);
        assert(result.kind.tag == 3 && result.kind.array);
        assert(keccak256(result.data) == keccak256(hex"abcd"));
    }

    function strings() external pure returns (string[] memory values) {
        values = new string[](2);
        values[0] = "bar";
        values[1] = "baz";
    }
}

contract StringConfig {
    mapping(uint256 => mapping(string => bytes)) stored;

    constructor(AbiEncodingTest source) {
        stored[1]["values"] = abi.encode(source.strings());
    }

    function reload(string[] memory values) external {
        stored[1]["values"] = abi.encode(values);
    }

    function get() external view returns (bytes memory) {
        return stored[1]["values"];
    }
}
