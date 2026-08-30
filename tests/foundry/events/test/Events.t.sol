// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Events.sol";

interface Vm {
    struct Log {
        bytes32[] topics;
        bytes data;
        address emitter;
    }

    function expectEmit(bool, bool, bool, bool) external;
    function getRecordedLogs() external returns (Log[] memory logs);
    function recordLogs() external;
}

contract EventsTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    bytes32 constant DEFAULT_TRIPLE_TOPIC =
        0x46700b4d40ac5c35af2c22dda2787a91eb567b06c924a8fb8ae9a05b20c08c21;
    bytes32 constant DEFAULT_DYNAMIC_TOPIC =
        0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563;
    Events public events;

    event SimpleEvent(uint256 value);
    event Transfer(address indexed from, address indexed to, uint256 value);

    function setUp() public {
        events = new Events();
    }

    function test_EmitSimple() public {
        vm.expectEmit(false, false, false, true);
        emit SimpleEvent(42);
        events.emitSimple(42);
    }

    function test_EmitTransfer() public {
        vm.expectEmit(true, true, false, true);
        emit Transfer(address(0x1), address(0x2), 100);
        events.emitTransfer(address(0x1), address(0x2), 100);
    }

    function test_EmitDefaultIndexedAggregate() public {
        vm.recordLogs();
        events.emitDefaultIndexedAggregate();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 2);
        require(logs[0].topics[1] == DEFAULT_TRIPLE_TOPIC);
    }

    function test_EmitDefaultIndexedDynamicAggregate() public {
        vm.recordLogs();
        events.emitDefaultIndexedDynamicAggregate();
        Vm.Log[] memory logs = vm.getRecordedLogs();
        require(logs.length == 1);
        require(logs[0].topics.length == 2);
        require(logs[0].topics[1] == DEFAULT_DYNAMIC_TOPIC);
    }
}
