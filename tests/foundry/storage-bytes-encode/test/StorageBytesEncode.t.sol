// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StorageBytesEncode.sol";

interface Vm {
    struct Log {
        bytes32[] topics;
        bytes data;
        address emitter;
    }

    function getRecordedLogs() external returns (Log[] memory logs);
    function recordLogs() external;
}

contract StorageBytesEncodeTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    StorageBytesEncode internal target;

    function setUp() public {
        target = new StorageBytesEncode();
    }

    function testEventData() public {
        vm.recordLogs();
        target.emitData();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 1);
        require(
            keccak256(logs[0].data)
                == keccak256(abi.encode("hello storage string that is longer than thirty-one bytes"))
        );
    }

    function testEventShortAndEmpty() public {
        vm.recordLogs();
        target.emitShort();
        target.emitEmpty();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 2);
        require(keccak256(logs[0].data) == keccak256(abi.encode("short")));
        require(keccak256(logs[1].data) == keccak256(abi.encode("")));
    }

    function testEventMixed() public {
        vm.recordLogs();
        target.emitMixed();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 2);
        require(logs[0].topics[1] == bytes32(uint256(7)));
        require(
            keccak256(logs[0].data)
                == keccak256(
                    abi.encode(
                        "hello storage string that is longer than thirty-one bytes",
                        uint8(9),
                        hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021"
                    )
                )
        );
    }

    function testEventReadsStorageAfterLaterArguments() public {
        vm.recordLogs();
        target.emitAfterMutation();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics[1] == bytes32(uint256(1)));
        require(
            keccak256(logs[0].data)
                == keccak256(
                    abi.encode(
                        "x",
                        uint8(9),
                        hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021"
                    )
                )
        );
    }

    function testStringAtSlot96() public {
        StorageBytesEncodeSlot96 slot96 = new StorageBytesEncodeSlot96();
        bytes memory expected = abi.encode("hello storage string that is longer than thirty-one bytes");
        require(keccak256(slot96.encoded()) == keccak256(expected));
        vm.recordLogs();
        slot96.emitData();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(keccak256(logs[0].data) == keccak256(expected));
    }
}
