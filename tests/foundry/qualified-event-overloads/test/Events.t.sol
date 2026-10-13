// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Events.sol";

interface Vm {
    struct Log {
        bytes32[] topics;
        bytes data;
        address emitter;
    }
    function recordLogs() external;
    function getRecordedLogs() external returns (Log[] memory);
}

contract EventsTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    bytes constant DATA = "abc";
    C c;

    function setUp() public {
        c = new C();
    }

    function test_fromLibrary() public {
        vm.recordLogs();
        c.fromLibrary(7, DATA);
        assertDerived(vm.getRecordedLogs());
    }

    function test_derived() public {
        vm.recordLogs();
        c.derived(7, DATA);
        assertDerived(vm.getRecordedLogs());
    }

    function test_base() public {
        vm.recordLogs();
        c.base(7, DATA);
        assertBase(vm.getRecordedLogs());
    }

    function test_unqualified() public {
        vm.recordLogs();
        c.unqualified(7, DATA);
        assertBase(vm.getRecordedLogs());
    }

    function assertDerived(Vm.Log[] memory logs) internal view {
        assert(logs.length == 1);
        assert(logs[0].emitter == address(c));
        assert(logs[0].topics.length == 1);
        assert(logs[0].topics[0] == keccak256("E(uint64,bytes)"));
        assert(keccak256(logs[0].data) == keccak256(abi.encode(uint64(7), DATA)));
    }

    function assertBase(Vm.Log[] memory logs) internal view {
        assert(logs.length == 1);
        assert(logs[0].emitter == address(c));
        assert(logs[0].topics.length == 2);
        assert(logs[0].topics[0] == keccak256("E(uint256,bytes)"));
        assert(logs[0].topics[1] == bytes32(uint256(7)));
        assert(keccak256(logs[0].data) == keccak256(abi.encode(DATA)));
    }
}
