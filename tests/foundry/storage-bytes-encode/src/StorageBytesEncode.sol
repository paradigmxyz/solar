// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract StorageBytesEncode {
    event Data(string data);
    event Mixed(uint256 indexed id, string first, uint8 middle, bytes second);

    string internal short = "short";
    string internal long = "hello storage string that is longer than thirty-one bytes";
    bytes internal wide = hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021";
    string internal empty;

    function emitData() external {
        emit Data(long);
    }

    function emitShort() external {
        emit Data(short);
    }

    function emitEmpty() external {
        emit Data(empty);
    }

    function emitMixed() external {
        emit Mixed(7, long, 9, wide);
    }

    function emitAfterMutation() external {
        emit Mixed(mutate(), long, 9, wide);
    }

    function mutate() internal returns (uint256) {
        long = "x";
        return 1;
    }
}

// The string lives at slot 96, the address of the zero slot in memory.
contract StorageBytesEncodeSlot96 {
    event Data(string data);

    uint256[96] internal gap;
    string internal value = "hello storage string that is longer than thirty-one bytes";

    function emitData() external {
        emit Data(value);
    }

    function encoded() external view returns (bytes memory) {
        return abi.encode(value);
    }
}
